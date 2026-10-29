//! Parser and processor for Traefik JSON access logs.

use std::sync::Arc;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use shipyard_observability::{
    build_entry, hash_visitor, LogContext, LogEntry, LogSource, Masker, MetricStore, Stream,
};
use crate::buffer::LogBuffer;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TraefikAccessLogRecord {
    #[serde(rename = "StartUTC")]
    pub start_utc: Option<DateTime<Utc>>,
    #[serde(rename = "ClientHost")]
    pub client_host: Option<String>,
    #[serde(rename = "DownstreamStatus")]
    pub status: Option<u16>,
    #[serde(rename = "DownstreamContentSize")]
    pub content_size: Option<u64>,
    #[serde(rename = "Duration")]
    pub duration_ns: Option<u64>,
    #[serde(rename = "RouterName")]
    pub router_name: Option<String>,
    #[serde(rename = "ServiceName")]
    pub service_name: Option<String>,
    #[serde(rename = "RequestPath")]
    pub request_path: Option<String>,
    #[serde(rename = "RequestMethod")]
    pub request_method: Option<String>,
    #[serde(rename = "OriginUserAgent")]
    pub user_agent: Option<String>,
    #[serde(rename = "request_traceparent")]
    pub traceparent: Option<String>,
    #[serde(flatten)]
    pub extra: Value,
}

pub struct TraefikTailer {
    daily_salt: Vec<u8>,
    metric_store: Arc<dyn MetricStore>,
    log_buffer: Option<Arc<LogBuffer>>,
    masker: Masker,
}

impl TraefikTailer {
    pub fn new(
        daily_salt: Vec<u8>,
        metric_store: Arc<dyn MetricStore>,
        log_buffer: Option<Arc<LogBuffer>>,
    ) -> Self {
        Self {
            daily_salt,
            metric_store,
            log_buffer,
            masker: Masker::default(),
        }
    }

    pub fn set_daily_salt(&mut self, salt: Vec<u8>) {
        self.daily_salt = salt;
    }

    /// Process a single Traefik access log JSON line.
    pub async fn process_line(&self, line: &str) -> Result<Option<LogEntry>, String> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let rec: TraefikAccessLogRecord = serde_json::from_str(trimmed)
            .map_err(|e| format!("invalid traefik json log line: {e}"))?;

        let ts = rec.start_utc.unwrap_or_else(Utc::now);
        // 1-minute bucket
        let bucket = ts - Duration::seconds(ts.timestamp() % 60) - Duration::nanoseconds(ts.timestamp_subsec_nanos() as i64);

        let status = rec.status.unwrap_or(200);
        let duration_ms = (rec.duration_ns.unwrap_or(0) / 1_000_000) as u32;
        let bytes = rec.content_size.unwrap_or(0);

        // Env / Project ID resolution: Traefik router names follow "<service-slug>@docker" or similar
        let env_id = rec
            .service_name
            .as_deref()
            .or(rec.router_name.as_deref())
            .unwrap_or("unknown");

        // Compute privacy-preserving visitor hash if IP and User-Agent are present
        let visitor_hash = match (&rec.client_host, &rec.user_agent) {
            (Some(ip), Some(ua)) if !ip.is_empty() && !self.daily_salt.is_empty() => {
                Some(hash_visitor(&self.daily_salt, ip, ua))
            }
            _ => None,
        };

        // Record metrics rollup
        self.metric_store
            .record_traffic_rollup(
                env_id,
                bucket,
                status,
                duration_ms,
                bytes,
                visitor_hash.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())?;

        // For 5xx errors, create an error log line for Traefik
        let maybe_log = if status >= 500 {
            let trace_id = rec.traceparent.as_deref().and_then(|tp| {
                shipyard_observability::parse::parse_traceparent(tp).map(|(t, _)| t)
            });

            let method = rec.request_method.as_deref().unwrap_or("GET");
            let path = rec.request_path.as_deref().unwrap_or("/");
            let raw_msg = format!(
                r#"{{"level":"error","msg":"{} {} -> {} ({}ms)","router":"{}","status":{}}}"#,
                method, path, status, duration_ms, env_id, status
            );

            let ctx = LogContext {
                source: Some(LogSource::Traefik),
                ..Default::default()
            };

            let mut entry = build_entry(&ctx, Stream::Stderr, ts, &raw_msg, &self.masker);
            if let Some(tid) = trace_id {
                entry.trace_id = Some(tid);
            }

            if let Some(buf) = &self.log_buffer {
                buf.push(entry.clone());
            }

            Some(entry)
        } else {
            None
        };

        Ok(maybe_log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;
    use uuid::Uuid;
    use shipyard_observability::{RetentionPolicy, StoreError};

    #[derive(Default)]
    struct MockMetricStore {
        calls: Mutex<Vec<(String, u16, Option<String>)>>,
    }

    #[async_trait]
    impl MetricStore for MockMetricStore {
        async fn record_traffic_rollup(
            &self,
            env_id: &str,
            _bucket: DateTime<Utc>,
            status_code: u16,
            _duration_ms: u32,
            _bytes: u64,
            visitor_hash: Option<&str>,
        ) -> Result<(), StoreError> {
            self.calls.lock().unwrap().push((
                env_id.to_string(),
                status_code,
                visitor_hash.map(str::to_string),
            ));
            Ok(())
        }

        async fn record_resource_sample(
            &self,
            _c_id: &str,
            _s_slug: &str,
            _p_id: Option<Uuid>,
            _cpu: f64,
            _mem: f64,
            _m_bytes: u64,
            _m_limit: u64,
        ) -> Result<(), StoreError> {
            Ok(())
        }

        async fn prune_metrics(&self, _policy: &RetentionPolicy) -> Result<shipyard_observability::PruneSummary, StoreError> {
            Ok(Default::default())
        }
    }

    #[tokio::test]
    async fn parses_200_access_line_records_metric() {
        let store = Arc::new(MockMetricStore::default());
        let tailer = TraefikTailer::new(b"test-salt-123".to_vec(), store.clone(), None);

        let line = r#"{
            "StartUTC": "2026-10-29T12:00:00Z",
            "ClientHost": "198.51.100.1",
            "DownstreamStatus": 200,
            "DownstreamContentSize": 1024,
            "Duration": 15000000,
            "RouterName": "web-router@docker",
            "ServiceName": "web-service@docker",
            "RequestPath": "/api/users",
            "RequestMethod": "GET",
            "OriginUserAgent": "Mozilla/5.0"
        }"#;

        let res = tailer.process_line(line).await.unwrap();
        assert!(res.is_none(), "200 OK should not generate error log line");

        let calls = store.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "web-service@docker");
        assert_eq!(calls[0].1, 200);
        assert!(calls[0].2.is_some(), "Visitor hash should be computed");
    }

    #[tokio::test]
    async fn parses_502_generates_error_log_line() {
        let store = Arc::new(MockMetricStore::default());
        let tailer = TraefikTailer::new(b"test-salt-123".to_vec(), store.clone(), None);

        let line = r#"{
            "StartUTC": "2026-10-29T12:00:00Z",
            "ClientHost": "198.51.100.1",
            "DownstreamStatus": 502,
            "DownstreamContentSize": 64,
            "Duration": 50000000,
            "ServiceName": "api-service",
            "RequestPath": "/checkout",
            "RequestMethod": "POST",
            "request_traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        }"#;

        let res = tailer.process_line(line).await.unwrap();
        assert!(res.is_some(), "502 must generate error log entry");
        let entry = res.unwrap();
        assert_eq!(entry.source, LogSource::Traefik);
        assert_eq!(entry.level, shipyard_observability::LogLevel::Error);
        assert_eq!(
            entry.trace_id.as_deref(),
            Some("4bf92f3577b34da6a3ce929d0e0e4736")
        );
        assert!(entry.message.as_str().contains("POST /checkout -> 502"));
    }
}
