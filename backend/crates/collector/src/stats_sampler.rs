//! Periodic resource stats sampler for containers.

use std::sync::Arc;
use uuid::Uuid;

use shipyard_observability::MetricStore;

#[derive(Debug, Clone)]
pub struct ContainerResourceMetrics {
    pub container_id: String,
    pub service_slug: String,
    pub project_id: Option<Uuid>,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub memory_bytes: u64,
    pub memory_limit_bytes: u64,
}

pub struct StatsSampler {
    metric_store: Arc<dyn MetricStore>,
}

impl StatsSampler {
    pub fn new(metric_store: Arc<dyn MetricStore>) -> Self {
        Self { metric_store }
    }

    pub async fn record_sample(&self, sample: ContainerResourceMetrics) -> Result<(), String> {
        self.metric_store
            .record_resource_sample(
                &sample.container_id,
                &sample.service_slug,
                sample.project_id,
                sample.cpu_percent,
                sample.memory_percent,
                sample.memory_bytes,
                sample.memory_limit_bytes,
            )
            .await
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use std::sync::Mutex;
    use shipyard_observability::{RetentionPolicy, StoreError};

    #[derive(Default)]
    struct MockMetricStore {
        samples: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl MetricStore for MockMetricStore {
        async fn record_traffic_rollup(
            &self,
            _env: &str,
            _bucket: DateTime<Utc>,
            _status: u16,
            _duration: u32,
            _bytes: u64,
            _vh: Option<&str>,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        async fn record_resource_sample(
            &self,
            container_id: &str,
            _service_slug: &str,
            _project_id: Option<Uuid>,
            _cpu: f64,
            _mem: f64,
            _bytes: u64,
            _limit: u64,
        ) -> Result<(), StoreError> {
            self.samples.lock().unwrap().push(container_id.to_string());
            Ok(())
        }

        async fn prune_metrics(&self, _policy: &RetentionPolicy) -> Result<shipyard_observability::PruneSummary, StoreError> {
            Ok(Default::default())
        }
    }

    #[tokio::test]
    async fn records_sample_successfully() {
        let store = Arc::new(MockMetricStore::default());
        let sampler = StatsSampler::new(store.clone());

        sampler
            .record_sample(ContainerResourceMetrics {
                container_id: "c1".into(),
                service_slug: "web".into(),
                project_id: None,
                cpu_percent: 12.5,
                memory_percent: 45.0,
                memory_bytes: 512 * 1024 * 1024,
                memory_limit_bytes: 1024 * 1024 * 1024,
            })
            .await
            .unwrap();

        assert_eq!(store.samples.lock().unwrap().len(), 1);
    }
}
