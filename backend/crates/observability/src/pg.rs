//! PostgreSQL implementation of [`LogStore`] and [`MetricStore`].

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::mask::{MaskedJson, MaskedText};
use crate::query::{Bind, LogPage, LogQuery, SqlQuery};
use crate::retention::{partition_bounds, partition_name_for_date, RetentionPolicy};
use crate::stats::{LogCountPoint, LogStatsQuery, LogStatsResult, StatBucket};
use crate::store::{LogStore, MetricStore, PruneSummary, StoreError};
use crate::types::{LogEntry, LogLevel, LogSource, StoredLogEntry, Stream};

pub struct PgLogStore {
    pool: PgPool,
}

impl PgLogStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl LogStore for PgLogStore {
    async fn write_entries(&self, entries: &[LogEntry]) -> Result<usize, StoreError> {
        if entries.is_empty() {
            return Ok(0);
        }

        // Batch insert in chunks of up to 500 rows
        const BATCH_SIZE: usize = 500;
        let mut total_inserted = 0;

        for chunk in entries.chunks(BATCH_SIZE) {
            let mut qb = sqlx::QueryBuilder::new(
                "INSERT INTO log_entries (ts, org_id, project_id, service_id, deployment_id, \
                 source, container_id, replica, stream, level, trace_id, span_id, message, fields) ",
            );

            qb.push_values(chunk, |mut b, e| {
                b.push_bind(e.ts)
                    .push_bind(e.org_id)
                    .push_bind(e.project_id)
                    .push_bind(e.service_id)
                    .push_bind(e.deployment_id)
                    .push_bind(e.source.as_str())
                    .push_bind(e.container_id.as_deref())
                    .push_bind(e.replica)
                    .push_bind(e.stream.as_str())
                    .push_bind(e.level.as_i16())
                    .push_bind(e.trace_id.as_deref())
                    .push_bind(e.span_id.as_deref())
                    .push_bind(e.message.as_str())
                    .push_bind(e.fields.as_ref().map(|f| f.as_value()));
            });

            let query = qb.build();
            query
                .execute(&self.pool)
                .await
                .map_err(|e| StoreError::Database(e.to_string()))?;

            total_inserted += chunk.len();
        }

        Ok(total_inserted)
    }

    async fn query_logs(&self, query: &LogQuery) -> Result<LogPage, StoreError> {
        let SqlQuery { sql, binds } = query.to_sql();
        let mut q = sqlx::query(&sql);

        for bind in binds {
            q = match bind {
                Bind::Uuid(u) => q.bind(u),
                Bind::Ts(ts) => q.bind(ts),
                Bind::Text(t) => q.bind(t),
                Bind::TextArray(arr) => q.bind(arr),
                Bind::SmallIntArray(arr) => q.bind(arr),
                Bind::Int(i) => q.bind(i),
                Bind::BigInt(bi) => q.bind(bi),
            };
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let id: i64 = row.try_get("id").map_err(|e| StoreError::Database(e.to_string()))?;
            let ts: DateTime<Utc> = row.try_get("ts").map_err(|e| StoreError::Database(e.to_string()))?;
            let org_id: Option<Uuid> = row.try_get("org_id").map_err(|e| StoreError::Database(e.to_string()))?;
            let project_id: Option<Uuid> = row.try_get("project_id").map_err(|e| StoreError::Database(e.to_string()))?;
            let service_id: Option<Uuid> = row.try_get("service_id").map_err(|e| StoreError::Database(e.to_string()))?;
            let deployment_id: Option<Uuid> = row.try_get("deployment_id").map_err(|e| StoreError::Database(e.to_string()))?;
            let source_str: String = row.try_get("source").map_err(|e| StoreError::Database(e.to_string()))?;
            let container_id: Option<String> = row.try_get("container_id").map_err(|e| StoreError::Database(e.to_string()))?;
            let replica: Option<i32> = row.try_get("replica").map_err(|e| StoreError::Database(e.to_string()))?;
            let stream_str: String = row.try_get("stream").map_err(|e| StoreError::Database(e.to_string()))?;
            let level_val: i16 = row.try_get("level").map_err(|e| StoreError::Database(e.to_string()))?;
            let trace_id: Option<String> = row.try_get("trace_id").map_err(|e| StoreError::Database(e.to_string()))?;
            let span_id: Option<String> = row.try_get("span_id").map_err(|e| StoreError::Database(e.to_string()))?;
            let message_str: String = row.try_get("message").map_err(|e| StoreError::Database(e.to_string()))?;
            let fields_val: Option<serde_json::Value> = row.try_get("fields").map_err(|e| StoreError::Database(e.to_string()))?;

            let source = source_str.parse().unwrap_or(LogSource::App);
            let stream = stream_str.parse().unwrap_or(Stream::Stdout);
            let level = LogLevel::from_i16(level_val).unwrap_or(LogLevel::Info);

            entries.push(StoredLogEntry {
                id,
                entry: LogEntry {
                    ts,
                    org_id,
                    project_id,
                    service_id,
                    deployment_id,
                    source,
                    container_id,
                    replica,
                    stream,
                    level,
                    trace_id,
                    span_id,
                    message: MaskedText::from_store(message_str),
                    fields: fields_val.map(MaskedJson::from_store),
                },
            });
        }

        Ok(LogPage::from_rows(entries, query.limit))
    }

    async fn query_stats(&self, query: &LogStatsQuery) -> Result<LogStatsResult, StoreError> {
        let date_trunc_arg = match query.bucket {
            StatBucket::Hour => "hour",
            StatBucket::Day => "day",
        };

        // Query aggregated log counts from log_counts_1h or direct from log_entries
        let mut qb = sqlx::QueryBuilder::new(
            "SELECT date_trunc(",
        );
        qb.push_bind(date_trunc_arg);
        qb.push(", ts) AS bucket_start, source, level, count(*) AS count \
                 FROM log_entries WHERE ts >= ");
        qb.push_bind(query.from);
        qb.push(" AND ts < ");
        qb.push_bind(query.to);

        if let Some(org_id) = query.org_id {
            qb.push(" AND org_id = ");
            qb.push_bind(org_id);
        }
        if let Some(project_id) = query.project_id {
            qb.push(" AND project_id = ");
            qb.push_bind(project_id);
        }
        if let Some(service_id) = query.service_id {
            qb.push(" AND service_id = ");
            qb.push_bind(service_id);
        }
        if !query.sources.is_empty() {
            let src_strs: Vec<String> = query.sources.iter().map(|s| s.as_str().to_string()).collect();
            qb.push(" AND source = ANY(");
            qb.push_bind(src_strs);
            qb.push(")");
        }
        if !query.levels.is_empty() {
            let lvl_i16s: Vec<i16> = query.levels.iter().map(|l| l.as_i16()).collect();
            qb.push(" AND level = ANY(");
            qb.push_bind(lvl_i16s);
            qb.push(")");
        }

        qb.push(" GROUP BY 1, 2, 3 ORDER BY 1 ASC");

        let rows = qb
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut total = 0u64;
        let mut errors = 0u64;
        let mut points = Vec::with_capacity(rows.len());

        for row in rows {
            let bucket_start: DateTime<Utc> = row.try_get("bucket_start").map_err(|e| StoreError::Database(e.to_string()))?;
            let source_str: String = row.try_get("source").map_err(|e| StoreError::Database(e.to_string()))?;
            let level_val: i16 = row.try_get("level").map_err(|e| StoreError::Database(e.to_string()))?;
            let count: i64 = row.try_get("count").map_err(|e| StoreError::Database(e.to_string()))?;

            let source = source_str.parse().unwrap_or(LogSource::App);
            let level = LogLevel::from_i16(level_val).unwrap_or(LogLevel::Info);
            let count = count.max(0) as u64;

            total += count;
            if level >= LogLevel::Error {
                errors += count;
            }

            points.push(LogCountPoint {
                bucket_start,
                source,
                level,
                count,
            });
        }

        Ok(LogStatsResult { total, errors, points })
    }

    async fn ensure_partitions(&self, dates: &[NaiveDate]) -> Result<(), StoreError> {
        for &date in dates {
            let name = partition_name_for_date(date);
            let (start, end) = partition_bounds(date);

            let sql = format!(
                "CREATE TABLE IF NOT EXISTS {name} PARTITION OF log_entries \
                 FOR VALUES FROM ('{}') TO ('{}')",
                start.to_rfc3339(),
                end.to_rfc3339()
            );

            sqlx::query(&sql)
                .execute(&self.pool)
                .await
                .map_err(|e| StoreError::Partition(format!("failed to ensure partition {name}: {e}")))?;
        }
        Ok(())
    }

    async fn prune(&self, policy: &RetentionPolicy) -> Result<PruneSummary, StoreError> {
        let cutoff = policy.log_cutoff(Utc::now());
        let cutoff_date = cutoff.date_naive();

        // 1. Find existing partitions older than cutoff_date
        let rows = sqlx::query(
            "SELECT inhrelid::regclass::text AS tablename \
             FROM pg_inherits \
             WHERE inhparent = 'log_entries'::regclass"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?;

        let mut partitions_dropped = 0;
        for row in rows {
            let tablename: String = row.try_get("tablename").map_err(|e| StoreError::Database(e.to_string()))?;
            // Table name format: log_entries_yYYYYmMMdDD
            if let Some(date_part) = tablename.strip_prefix("log_entries_y") {
                if date_part.len() == 11 && date_part.contains('m') && date_part.contains('d') {
                    let parts: Vec<&str> = date_part.split(['m', 'd']).collect();
                    if parts.len() == 3 {
                        if let (Ok(y), Ok(m), Ok(d)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>(), parts[2].parse::<u32>()) {
                            if let Some(date) = NaiveDate::from_ymd_opt(y, m, d) {
                                if date < cutoff_date {
                                    let drop_sql = format!("DROP TABLE IF EXISTS {tablename}");
                                    if let Err(e) = sqlx::query(&drop_sql).execute(&self.pool).await {
                                        tracing::warn!("Failed to drop partition {tablename}: {e}");
                                    } else {
                                        partitions_dropped += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. Prune records in default partition that might be older than cutoff
        sqlx::query("DELETE FROM log_entries_default WHERE ts < $1")
            .bind(cutoff)
            .execute(&self.pool)
            .await
            .ok();

        // 3. Prune visitor hashes
        let v_res = sqlx::query("DELETE FROM visitor_hashes WHERE bucket < $1")
            .bind(policy.visitor_hash_cutoff(Utc::now()))
            .execute(&self.pool)
            .await;
        let visitor_hashes_deleted = v_res.map(|r| r.rows_affected()).unwrap_or(0);

        // 4. Prune resource samples
        let r_res = sqlx::query("DELETE FROM resource_samples_1m WHERE bucket < $1")
            .bind(policy.raw_resource_cutoff(Utc::now()))
            .execute(&self.pool)
            .await;
        let resource_samples_deleted = r_res.map(|r| r.rows_affected()).unwrap_or(0);

        let rollup_res = sqlx::query("DELETE FROM resource_samples_rollup WHERE bucket < $1")
            .bind(policy.rollup_resource_cutoff(Utc::now()))
            .execute(&self.pool)
            .await;
        let rollups_deleted = rollup_res.map(|r| r.rows_affected()).unwrap_or(0);

        Ok(PruneSummary {
            partitions_dropped,
            visitor_hashes_deleted,
            resource_samples_deleted,
            rollups_deleted,
        })
    }
}

pub struct PgMetricStore {
    pool: PgPool,
}

impl PgMetricStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MetricStore for PgMetricStore {
    async fn record_traffic_rollup(
        &self,
        env_id: &str,
        bucket: DateTime<Utc>,
        status_code: u16,
        duration_ms: u32,
        bytes: u64,
        visitor_hash: Option<&str>,
    ) -> Result<(), StoreError> {
        let project_id = Uuid::parse_str(env_id).unwrap_or_else(|_| Uuid::nil());
        let (s2, s3, s4, s5) = match status_code {
            200..=299 => (1i64, 0i64, 0i64, 0i64),
            300..=399 => (0, 1, 0, 0),
            400..=499 => (0, 0, 1, 0),
            500..=599 => (0, 0, 0, 1),
            _ => (0, 0, 0, 0),
        };

        sqlx::query(
            "INSERT INTO request_rollups_1m (project_id, environment, bucket, requests, s2xx, s3xx, s4xx, s5xx, bytes) \
             VALUES ($1, $2, $3, 1, $4, $5, $6, $7, $8) \
             ON CONFLICT (project_id, environment, bucket) DO UPDATE SET \
                requests = request_rollups_1m.requests + 1, \
                s2xx = request_rollups_1m.s2xx + EXCLUDED.s2xx, \
                s3xx = request_rollups_1m.s3xx + EXCLUDED.s3xx, \
                s4xx = request_rollups_1m.s4xx + EXCLUDED.s4xx, \
                s5xx = request_rollups_1m.s5xx + EXCLUDED.s5xx, \
                bytes = request_rollups_1m.bytes + EXCLUDED.bytes"
        )
        .bind(project_id)
        .bind(env_id)
        .bind(bucket)
        .bind(s2)
        .bind(s3)
        .bind(s4)
        .bind(s5)
        .bind(bytes as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?;

        if let Some(hash) = visitor_hash {
            sqlx::query(
                "INSERT INTO visitor_hashes (project_id, environment, bucket, hash) \
                 VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (project_id, environment, bucket, hash) DO NOTHING"
            )
            .bind(project_id)
            .bind(env_id)
            .bind(bucket)
            .bind(hash)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Database(e.to_string()))?;
        }

        let _ = duration_ms;
        Ok(())
    }

    async fn record_resource_sample(
        &self,
        container_id: &str,
        service_slug: &str,
        project_id: Option<Uuid>,
        cpu_percent: f64,
        memory_percent: f64,
        memory_bytes: u64,
        memory_limit_bytes: u64,
    ) -> Result<(), StoreError> {
        let bucket = Utc::now();
        sqlx::query(
            "INSERT INTO resource_samples_1m (project_id, service_slug, container_id, bucket, \
             cpu_percent, memory_percent, memory_bytes, memory_limit_bytes) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(project_id)
        .bind(service_slug)
        .bind(container_id)
        .bind(bucket)
        .bind(cpu_percent)
        .bind(memory_percent)
        .bind(memory_bytes as i64)
        .bind(memory_limit_bytes as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Database(e.to_string()))?;

        Ok(())
    }

    async fn prune_metrics(&self, policy: &RetentionPolicy) -> Result<PruneSummary, StoreError> {
        let now = Utc::now();
        let v_res = sqlx::query("DELETE FROM visitor_hashes WHERE bucket < $1")
            .bind(policy.visitor_hash_cutoff(now))
            .execute(&self.pool)
            .await;
        let visitor_hashes_deleted = v_res.map(|r| r.rows_affected()).unwrap_or(0);

        let r_res = sqlx::query("DELETE FROM resource_samples_1m WHERE bucket < $1")
            .bind(policy.raw_resource_cutoff(now))
            .execute(&self.pool)
            .await;
        let resource_samples_deleted = r_res.map(|r| r.rows_affected()).unwrap_or(0);

        let rollup_res = sqlx::query("DELETE FROM resource_samples_rollup WHERE bucket < $1")
            .bind(policy.rollup_resource_cutoff(now))
            .execute(&self.pool)
            .await;
        let rollups_deleted = rollup_res.map(|r| r.rows_affected()).unwrap_or(0);

        Ok(PruneSummary {
            partitions_dropped: 0,
            visitor_hashes_deleted,
            resource_samples_deleted,
            rollups_deleted,
        })
    }
}
