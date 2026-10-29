//! Storage traits and in-memory mock for logs and metrics.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::query::{LogPage, LogQuery};
use crate::retention::RetentionPolicy;
use crate::stats::{LogStatsQuery, LogStatsResult};
use crate::types::{LogEntry, StoredLogEntry};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(String),
    #[error("serialization error: {0}")]
    Serialization(String),
    #[error("partition error: {0}")]
    Partition(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruneSummary {
    pub partitions_dropped: usize,
    pub visitor_hashes_deleted: u64,
    pub resource_samples_deleted: u64,
    pub rollups_deleted: u64,
}

/// Abstract log storage. Keeps ClickHouse / Loki / in-memory testing possible.
#[async_trait]
pub trait LogStore: Send + Sync {
    /// Ingest a batch of masked log entries.
    async fn write_entries(&self, entries: &[LogEntry]) -> Result<usize, StoreError>;

    /// Query logs with pagination and scoping.
    async fn query_logs(&self, query: &LogQuery) -> Result<LogPage, StoreError>;

    /// Query aggregated log counts for stats / charts.
    async fn query_stats(&self, query: &LogStatsQuery) -> Result<LogStatsResult, StoreError>;

    /// Ensure required daily partition tables exist.
    async fn ensure_partitions(&self, dates: &[NaiveDate]) -> Result<(), StoreError>;

    /// Prune partitions and records exceeding retention policy.
    async fn prune(&self, policy: &RetentionPolicy) -> Result<PruneSummary, StoreError>;
}

/// Thread-safe in-memory store for unit tests and local mock runs.
#[derive(Clone, Default)]
pub struct MemLogStore {
    id_gen: Arc<AtomicI64>,
    entries: Arc<RwLock<Vec<StoredLogEntry>>>,
}

impl MemLogStore {
    pub fn new() -> Self {
        Self {
            id_gen: Arc::new(AtomicI64::new(1)),
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.entries.read().await.is_empty()
    }
}

#[async_trait]
impl LogStore for MemLogStore {
    async fn write_entries(&self, entries: &[LogEntry]) -> Result<usize, StoreError> {
        let mut list = self.entries.write().await;
        let count = entries.len();
        for entry in entries {
            let id = self.id_gen.fetch_add(1, Ordering::Relaxed);
            list.push(StoredLogEntry {
                id,
                entry: entry.clone(),
            });
        }
        Ok(count)
    }

    async fn query_logs(&self, query: &LogQuery) -> Result<LogPage, StoreError> {
        let list = self.entries.read().await;
        let mut matched: Vec<StoredLogEntry> = list
            .iter()
            .filter(|r| query.matches(r))
            .cloned()
            .collect();

        // Sort ts DESC, id DESC
        matched.sort_by(|a, b| b.entry.ts.cmp(&a.entry.ts).then(b.id.cmp(&a.id)));

        Ok(LogPage::from_rows(matched, query.limit))
    }

    async fn query_stats(&self, query: &LogStatsQuery) -> Result<LogStatsResult, StoreError> {
        let list = self.entries.read().await;
        let mut total = 0u64;
        let mut errors = 0u64;

        for r in list.iter() {
            let e = &r.entry;
            if e.ts < query.from || e.ts >= query.to {
                continue;
            }
            if let Some(org) = query.org_id {
                if e.org_id != Some(org) {
                    continue;
                }
            }
            if !query.sources.is_empty() && !query.sources.contains(&e.source) {
                continue;
            }
            if !query.levels.is_empty() && !query.levels.contains(&e.level) {
                continue;
            }
            if let Some(pid) = query.project_id {
                if e.project_id != Some(pid) {
                    continue;
                }
            }
            if let Some(sid) = query.service_id {
                if e.service_id != Some(sid) {
                    continue;
                }
            }

            total += 1;
            if e.level >= crate::types::LogLevel::Error {
                errors += 1;
            }
        }

        Ok(LogStatsResult {
            total,
            errors,
            points: Vec::new(),
        })
    }

    async fn ensure_partitions(&self, _dates: &[NaiveDate]) -> Result<(), StoreError> {
        Ok(())
    }

    async fn prune(&self, policy: &RetentionPolicy) -> Result<PruneSummary, StoreError> {
        let cutoff = policy.log_cutoff(Utc::now());
        let mut list = self.entries.write().await;
        let before_len = list.len();
        list.retain(|r| r.entry.ts >= cutoff);
        let dropped = before_len - list.len();
        Ok(PruneSummary {
            partitions_dropped: if dropped > 0 { 1 } else { 0 },
            visitor_hashes_deleted: 0,
            resource_samples_deleted: 0,
            rollups_deleted: 0,
        })
    }
}

/// Abstract metrics storage for rollups, visitor hashes, and container resources.
#[async_trait]
pub trait MetricStore: Send + Sync {
    async fn record_traffic_rollup(
        &self,
        env_id: &str,
        bucket: DateTime<Utc>,
        status_code: u16,
        duration_ms: u32,
        bytes: u64,
        visitor_hash: Option<&str>,
    ) -> Result<(), StoreError>;

    async fn record_resource_sample(
        &self,
        container_id: &str,
        service_slug: &str,
        project_id: Option<Uuid>,
        cpu_percent: f64,
        memory_percent: f64,
        memory_bytes: u64,
        memory_limit_bytes: u64,
    ) -> Result<(), StoreError>;

    async fn prune_metrics(&self, policy: &RetentionPolicy) -> Result<PruneSummary, StoreError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::Masker;
    use crate::types::{LogContext, LogLevel, LogSource, Stream};
    use chrono::Duration;

    #[tokio::test]
    async fn mem_log_store_write_query_prune() {
        let store = MemLogStore::new();
        assert!(store.is_empty().await);

        let now = Utc::now();
        let masker = Masker::default();

        let ctx1 = LogContext {
            org_id: Some(Uuid::from_u128(1)),
            project_id: Some(Uuid::from_u128(10)),
            service_id: Some(Uuid::from_u128(100)),
            source: Some(LogSource::App),
            ..Default::default()
        };

        let e1 = crate::ingest::build_entry(
            &ctx1,
            Stream::Stdout,
            now - Duration::minutes(10),
            "healthy app start",
            &masker,
        );
        let e2 = crate::ingest::build_entry(
            &ctx1,
            Stream::Stderr,
            now - Duration::minutes(5),
            "{\"level\":\"error\",\"msg\":\"connection refused\"}",
            &masker,
        );

        let written = store.write_entries(&[e1, e2]).await.unwrap();
        assert_eq!(written, 2);
        assert_eq!(store.len().await, 2);

        // Query by Org scope
        let query = LogQuery::new(
            crate::query::Scope::Org(Uuid::from_u128(1)),
            crate::query::LogQueryParams::default(),
            now,
        )
        .unwrap();
        let page = store.query_logs(&query).await.unwrap();
        assert_eq!(page.entries.len(), 2);
        // Ordered ts DESC
        assert_eq!(page.entries[0].entry.level, LogLevel::Error);

        // Stats query
        let stats_q = LogStatsQuery::new(
            Some(Uuid::from_u128(1)),
            crate::stats::LogStatsParams::default(),
            now,
        )
        .unwrap();
        let stats = store.query_stats(&stats_q).await.unwrap();
        assert_eq!(stats.total, 2);
        assert_eq!(stats.errors, 1);

        // Pruning with strict policy
        let strict_policy = RetentionPolicy {
            retention_days: 0,
            ..Default::default()
        };
        let summary = store.prune(&strict_policy).await.unwrap();
        assert_eq!(summary.partitions_dropped, 1);
        assert_eq!(store.len().await, 0);
    }
}
