use rumqttc::{AsyncClient, QoS};
use shipyard_common::error::{AppError, AppResult};
use shipyard_common::types::MqttPayload;

/// MQTT publisher for fire-and-forget event publishing.
///
/// - Log lines: QoS 0 (at most once)
/// - Status changes: QoS 1 (at least once)
pub struct MqttPublisher {
    client: AsyncClient,
}

impl MqttPublisher {
    pub fn new(client: AsyncClient) -> Self {
        Self { client }
    }

    /// Publish a message to a topic with QoS 0 (fire-and-forget).
    pub async fn publish_log(&self, topic: &str, payload: &MqttPayload) -> AppResult<()> {
        let data = serde_json::to_vec(payload)
            .map_err(|e| AppError::Mqtt(format!("Serialization error: {e}")))?;

        self.client
            .publish(topic, QoS::AtMostOnce, false, data)
            .await
            .map_err(|e| AppError::Mqtt(format!("Publish error: {e}")))?;

        Ok(())
    }

    /// Publish a message to a topic with QoS 1 (at least once).
    pub async fn publish_status(&self, topic: &str, payload: &MqttPayload) -> AppResult<()> {
        let data = serde_json::to_vec(payload)
            .map_err(|e| AppError::Mqtt(format!("Serialization error: {e}")))?;

        self.client
            .publish(topic, QoS::AtLeastOnce, false, data)
            .await
            .map_err(|e| AppError::Mqtt(format!("Publish error: {e}")))?;

        Ok(())
    }
}
