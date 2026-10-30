//! Long-running collector tasks: follow every local container's log stream into
//! the [`LogBuffer`], persist per-container cursors, and keep daily log
//! partitions + retention up to date.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use bollard::container::{ListContainersOptions, LogOutput, LogsOptions};
use bollard::system::EventsOptions;
use bollard::Docker;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use sqlx::PgPool;
use tokio::sync::Mutex;

use shipyard_observability::retention::required_partition_dates;
use shipyard_observability::{LogSource, LogStore, PgLogStore, RetentionPolicy, Stream};

use crate::buffer::{LogBuffer, DEFAULT_BATCH_SIZE, DEFAULT_CHANNEL_CAPACITY, DEFAULT_FLUSH_INTERVAL};
use crate::docker_follower::{extract_metadata, ContainerMetadata, LogIngestEngine};

/// How far back to read a container's logs the first time we see it.
const INITIAL_BACKFILL: chrono::Duration = chrono::Duration::minutes(15);
/// Partitions created ahead of today.
const PARTITION_ADVANCE_DAYS: u32 = 3;
const CURSOR_FLUSH_INTERVAL: Duration = Duration::from_secs(10);
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60 * 60);

/// Start every collector task. Returns immediately; tasks run until the process exits.
pub async fn spawn(docker: Docker, pool: PgPool, label_prefix: String) {
    let store = Arc::new(PgLogStore::new(pool.clone()));

    // Partitions must exist before the first batch lands, otherwise rows go to
    // the default partition and block creating that day's partition later.
    run_maintenance(store.as_ref()).await;
    {
        let store = store.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(MAINTENANCE_INTERVAL);
            tick.tick().await;
            loop {
                tick.tick().await;
                run_maintenance(store.as_ref()).await;
            }
        });
    }

    let (buffer, _flush_handle) = LogBuffer::new(
        store.clone(),
        DEFAULT_CHANNEL_CAPACITY,
        DEFAULT_BATCH_SIZE,
        DEFAULT_FLUSH_INTERVAL,
    );
    let engine = Arc::new(LogIngestEngine::new(Arc::new(buffer)));

    {
        let engine = engine.clone();
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(CURSOR_FLUSH_INTERVAL);
            loop {
                tick.tick().await;
                flush_cursors(&pool, &engine).await;
            }
        });
    }

    let follower = Follower {
        docker,
        pool,
        label_prefix,
        engine,
        active: Arc::new(Mutex::new(HashSet::new())),
    };
    tokio::spawn(async move {
        let mut backoff = Duration::from_secs(1);
        loop {
            match follower.run().await {
                Ok(()) => backoff = Duration::from_secs(1),
                Err(e) => tracing::warn!("Log collector event stream ended: {e}. Restarting in {}s", backoff.as_secs()),
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    });

    tracing::info!("Observability collector started");
}

async fn run_maintenance(store: &PgLogStore) {
    let dates = required_partition_dates(Utc::now().date_naive(), PARTITION_ADVANCE_DAYS);
    if let Err(e) = store.ensure_partitions(&dates).await {
        tracing::error!("Failed to ensure log partitions: {e}");
    }
    match store.prune(&RetentionPolicy::default()).await {
        Ok(s) if s.partitions_dropped > 0 => {
            tracing::info!(dropped = s.partitions_dropped, "Pruned expired log partitions")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Log retention prune failed: {e}"),
    }
}

async fn flush_cursors(pool: &PgPool, engine: &LogIngestEngine) {
    let snapshot = engine.cursors().snapshot().await;
    if snapshot.is_empty() {
        return;
    }
    let (ids, ts): (Vec<String>, Vec<DateTime<Utc>>) = snapshot.into_iter().unzip();
    if let Err(e) = sqlx::query(
        "INSERT INTO log_cursors (container_id, last_seen, updated_at) \
         SELECT * , NOW() FROM UNNEST($1::text[], $2::timestamptz[]) \
         ON CONFLICT (container_id) DO UPDATE SET last_seen = EXCLUDED.last_seen, updated_at = NOW()",
    )
    .bind(&ids)
    .bind(&ts)
    .execute(pool)
    .await
    {
        tracing::warn!("Failed to persist log cursors: {e}");
    }
}

#[derive(Clone)]
struct Follower {
    docker: Docker,
    pool: PgPool,
    label_prefix: String,
    engine: Arc<LogIngestEngine>,
    /// Container ids with a live follow task.
    active: Arc<Mutex<HashSet<String>>>,
}

impl Follower {
    /// Attach to every running container, then attach to each one that starts.
    /// Returns when the Docker event stream ends.
    async fn run(&self) -> Result<(), bollard::errors::Error> {
        let mut filters = HashMap::new();
        filters.insert("type".to_string(), vec!["container".to_string()]);
        filters.insert("event".to_string(), vec!["start".to_string()]);
        // Subscribe before listing so a container starting in between is not missed.
        let mut events = self.docker.events(Some(EventsOptions::<String> {
            filters,
            ..Default::default()
        }));

        let running = self
            .docker
            .list_containers(Some(ListContainersOptions::<String> {
                all: false,
                ..Default::default()
            }))
            .await?;
        for c in running {
            let (Some(id), Some(name)) = (c.id, c.names.and_then(|n| n.into_iter().next())) else {
                continue;
            };
            self.attach(extract_metadata(&id, &name, &c.labels.unwrap_or_default(), &self.label_prefix));
        }

        while let Some(ev) = events.next().await {
            let ev = ev?;
            let Some(actor) = ev.actor else { continue };
            let Some(id) = actor.id else { continue };
            let mut attrs = actor.attributes.unwrap_or_default();
            let name = attrs.remove("name").unwrap_or_default();
            // Container labels arrive as plain event attributes.
            self.attach(extract_metadata(&id, &name, &attrs, &self.label_prefix));
        }
        Ok(())
    }

    fn attach(&self, meta: ContainerMetadata) {
        let this = self.clone();
        tokio::spawn(async move {
            if !this.active.lock().await.insert(meta.container_id.clone()) {
                return;
            }
            this.follow(&meta).await;
            this.active.lock().await.remove(&meta.container_id);
        });
    }

    async fn follow(&self, meta: &ContainerMetadata) {
        let cursor = match self.engine.cursors().get(&meta.container_id).await {
            Some(ts) => Some(ts),
            None => sqlx::query_scalar::<_, DateTime<Utc>>(
                "SELECT last_seen FROM log_cursors WHERE container_id = $1",
            )
            .bind(&meta.container_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten(),
        };
        let since = cursor.unwrap_or_else(|| Utc::now() - INITIAL_BACKFILL);

        tracing::debug!(container = %meta.name, source = %meta.context.source().as_str(), "Following container logs");

        let mut stream = self.docker.logs(
            &meta.container_id,
            Some(LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                timestamps: true,
                since: since.timestamp(),
                tail: "all".to_string(),
                ..Default::default()
            }),
        );

        while let Some(chunk) = stream.next().await {
            let (stream_kind, bytes) = match chunk {
                Ok(LogOutput::StdErr { message }) => (Stream::Stderr, message),
                Ok(LogOutput::StdOut { message }) | Ok(LogOutput::Console { message }) => {
                    (Stream::Stdout, message)
                }
                Ok(LogOutput::StdIn { .. }) => continue,
                Err(e) => {
                    tracing::debug!(container = %meta.name, "Log stream ended: {e}");
                    break;
                }
            };

            for line in String::from_utf8_lossy(&bytes).split('\n') {
                let Some((ts, msg)) = split_timestamp(line) else { continue };
                // `since` has one-second resolution; skip lines we already stored.
                if cursor.is_some_and(|c| ts <= c) {
                    continue;
                }
                if meta.context.source() == LogSource::Traefik && is_quiet_access_line(msg) {
                    continue;
                }
                self.engine.handle_line(meta, stream_kind, ts, msg).await;
            }
        }
    }
}

/// Split a Docker `timestamps=true` line (`<RFC3339Nano> <message>`).
fn split_timestamp(line: &str) -> Option<(DateTime<Utc>, &str)> {
    let line = line.trim_end_matches('\r');
    if line.is_empty() {
        return None;
    }
    let (ts, msg) = line.split_once(' ').unwrap_or((line, ""));
    let ts = DateTime::parse_from_rfc3339(ts).ok()?.with_timezone(&Utc);
    if msg.trim().is_empty() {
        return None;
    }
    Some((ts, msg))
}

/// Traefik writes one JSON access-log record per request to stdout. Storing
/// every one would drown the platform logs, so only 5xx responses are kept.
fn is_quiet_access_line(msg: &str) -> bool {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(msg) else {
        return false;
    };
    match v.get("DownstreamStatus").and_then(|s| s.as_u64()) {
        Some(status) => status < 500,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_docker_timestamp_prefix() {
        let (ts, msg) = split_timestamp("2026-10-30T08:15:02.123456789Z hello world").unwrap();
        assert_eq!(ts.to_rfc3339(), "2026-10-30T08:15:02.123456789+00:00");
        assert_eq!(msg, "hello world");
        assert!(split_timestamp("").is_none());
        assert!(split_timestamp("2026-10-30T08:15:02Z ").is_none());
        assert!(split_timestamp("not-a-timestamp hello").is_none());
    }

    #[test]
    fn drops_only_non_error_access_lines() {
        assert!(is_quiet_access_line(r#"{"DownstreamStatus":200,"RequestPath":"/"}"#));
        assert!(!is_quiet_access_line(r#"{"DownstreamStatus":502,"RequestPath":"/"}"#));
        assert!(!is_quiet_access_line(r#"{"level":"error","msg":"router not found"}"#));
        assert!(!is_quiet_access_line("plain text"));
    }
}
