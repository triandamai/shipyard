use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use shipyard_common::types::MqttPayload;

use crate::AppState;

const SPIKE_TOPIC: &str = "platform/alerts/spike";

/// Spike event received from a node agent.
#[derive(Debug, Deserialize)]
pub struct SpikeEvent {
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
    pub container_id: Option<String>,
    pub node_id: String,
    pub ts: u64,
}

pub fn internal_routes() -> Router<AppState> {
    Router::new().route("/node-agent/spike", post(receive_spike))
}

/// POST /internal/node-agent/spike
/// Called by node agents when a metric crosses a threshold.
/// Auth: X-Agent-Token header.
async fn receive_spike(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(event): Json<SpikeEvent>,
) -> StatusCode {
    let token = headers
        .get("x-agent-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if token != state.config.node_agent_token {
        return StatusCode::UNAUTHORIZED;
    }

    let cooldown = state.config.alert_cooldown_secs as f64;

    // Insert only if no alert fired for this (metric, node, container) within the cooldown window.
    let inserted = sqlx::query_scalar::<_, bool>(
        r#"
        INSERT INTO resource_alerts (metric, value, threshold, container_id, node_id)
        SELECT $1, $2, $3, $4, $5
        WHERE NOT EXISTS (
            SELECT 1 FROM resource_alerts
            WHERE metric = $1
              AND node_id = $5
              AND container_id IS NOT DISTINCT FROM $4
              AND fired_at > NOW() - ($6 || ' seconds')::INTERVAL
        )
        RETURNING true
        "#,
    )
    .bind(&event.metric)
    .bind(event.value)
    .bind(event.threshold)
    .bind(&event.container_id)
    .bind(&event.node_id)
    .bind(cooldown.to_string())
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)
    .unwrap_or(false);

    if inserted {
        let payload = MqttPayload::new("resource.spike").with_meta(serde_json::json!({
            "metric":       event.metric,
            "value":        event.value,
            "threshold":    event.threshold,
            "container_id": event.container_id,
            "node_id":      event.node_id,
            "ts":           event.ts,
        }));
        state.mqtt.publish_status(SPIKE_TOPIC, &payload).await.ok();
    }

    StatusCode::OK
}
