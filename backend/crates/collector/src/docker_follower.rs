//! Docker container log follower with cursor persistence and label extraction.

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use shipyard_observability::{
    build_entry, LogContext, LogSource, Masker, Stream,
};
use crate::buffer::LogBuffer;

/// Extracted metadata from container labels and name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerMetadata {
    pub container_id: String,
    pub name: String,
    pub context: LogContext,
}

pub fn extract_metadata(
    container_id: &str,
    name: &str,
    labels: &HashMap<String, String>,
    label_prefix: &str,
) -> ContainerMetadata {
    let clean_name = name.trim_start_matches('/');

    // Check platform container names first
    let platform_source = if clean_name.contains("traefik") {
        Some(LogSource::Traefik)
    } else if clean_name.contains("postgres") {
        Some(LogSource::Postgres)
    } else if clean_name.contains("backend") || clean_name.contains("shipyard-api") {
        Some(LogSource::Backend)
    } else if clean_name.contains("frontend") {
        Some(LogSource::Frontend)
    } else if clean_name.contains("collector") {
        Some(LogSource::Collector)
    } else {
        None
    };

    if let Some(src) = platform_source {
        return ContainerMetadata {
            container_id: container_id.to_string(),
            name: clean_name.to_string(),
            context: LogContext {
                org_id: None,
                project_id: None,
                service_id: None,
                deployment_id: None,
                source: Some(src),
                container_id: Some(container_id.to_string()),
                replica: None,
            },
        };
    }

    // Extract shipyard labels: <prefix>.org_id, <prefix>.service_id, etc.
    let get_uuid = |k: &str| {
        labels
            .get(&format!("{label_prefix}.{k}"))
            .or_else(|| labels.get(&format!("shipyard.{k}")))
            .or_else(|| labels.get(k))
            .and_then(|s| Uuid::parse_str(s).ok())
    };

    let org_id = get_uuid("org_id");
    let project_id = get_uuid("project_id");
    let service_id = get_uuid("service_id");
    let deployment_id = get_uuid("deployment_id");

    let replica = labels
        .get(&format!("{label_prefix}.replica"))
        .or_else(|| labels.get("shipyard.replica"))
        .or_else(|| labels.get("replica"))
        .and_then(|s| s.parse::<i32>().ok());

    let source = if labels.contains_key(&format!("{label_prefix}.managed"))
        || labels.contains_key("shipyard.managed")
        || service_id.is_some()
    {
        LogSource::App
    } else {
        LogSource::App
    };

    ContainerMetadata {
        container_id: container_id.to_string(),
        name: clean_name.to_string(),
        context: LogContext {
            org_id,
            project_id,
            service_id,
            deployment_id,
            source: Some(source),
            container_id: Some(container_id.to_string()),
            replica,
        },
    }
}

pub struct CursorTracker {
    cursors: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
}

impl Default for CursorTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorTracker {
    pub fn new() -> Self {
        Self {
            cursors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn get(&self, container_id: &str) -> Option<DateTime<Utc>> {
        self.cursors.read().await.get(container_id).copied()
    }

    pub async fn update(&self, container_id: &str, ts: DateTime<Utc>) {
        self.cursors.write().await.insert(container_id.to_string(), ts);
    }
}

/// Follower engine that ingests lines from container output.
pub struct LogIngestEngine {
    cursors: CursorTracker,
    buffer: Arc<LogBuffer>,
    masker: Masker,
}

impl LogIngestEngine {
    pub fn new(buffer: Arc<LogBuffer>) -> Self {
        Self {
            cursors: CursorTracker::new(),
            buffer,
            masker: Masker::default(),
        }
    }

    pub async fn handle_line(
        &self,
        meta: &ContainerMetadata,
        stream: Stream,
        ts: DateTime<Utc>,
        raw_line: &str,
    ) {
        let entry = build_entry(&meta.context, stream, ts, raw_line, &self.masker);
        self.cursors.update(&meta.container_id, ts).await;
        self.buffer.push(entry);
    }

    pub fn cursors(&self) -> &CursorTracker {
        &self.cursors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use shipyard_observability::MemLogStore;

    #[test]
    fn extracts_platform_container_metadata() {
        let labels = HashMap::new();
        let meta = extract_metadata("c123", "/shipyard-traefik", &labels, "shipyard");
        assert_eq!(meta.context.source(), LogSource::Traefik);
        assert_eq!(meta.context.org_id, None);

        let meta2 = extract_metadata("c456", "/shipyard-postgres", &labels, "shipyard");
        assert_eq!(meta2.context.source(), LogSource::Postgres);
        assert_eq!(meta2.context.org_id, None);

        let meta3 = extract_metadata("c789", "/shipyard-backend", &labels, "shipyard");
        assert_eq!(meta3.context.source(), LogSource::Backend);
        assert_eq!(meta3.context.org_id, None);
    }

    #[test]
    fn extracts_tenant_container_labels() {
        let org_id = Uuid::new_v4();
        let proj_id = Uuid::new_v4();
        let svc_id = Uuid::new_v4();

        let mut labels = HashMap::new();
        labels.insert("shipyard.managed".into(), "true".into());
        labels.insert("shipyard.org_id".into(), org_id.to_string());
        labels.insert("shipyard.project_id".into(), proj_id.to_string());
        labels.insert("shipyard.service_id".into(), svc_id.to_string());
        labels.insert("shipyard.replica".into(), "3".into());

        let meta = extract_metadata("capp", "/app_container_1", &labels, "shipyard");
        assert_eq!(meta.context.source(), LogSource::App);
        assert_eq!(meta.context.org_id, Some(org_id));
        assert_eq!(meta.context.project_id, Some(proj_id));
        assert_eq!(meta.context.service_id, Some(svc_id));
        assert_eq!(meta.context.replica, Some(3));
    }

    #[tokio::test]
    async fn ingests_line_and_advances_cursor() {
        let store = Arc::new(MemLogStore::new());
        let (buffer, _handle) = LogBuffer::new(store.clone(), 100, 10, Duration::from_millis(100));
        let engine = LogIngestEngine::new(Arc::new(buffer));

        let meta = extract_metadata("c1", "/app", &HashMap::new(), "shipyard");
        let now = Utc::now();

        engine.handle_line(&meta, Stream::Stdout, now, "hello from app").await;

        let cursor = engine.cursors().get("c1").await;
        assert_eq!(cursor, Some(now));
    }
}
