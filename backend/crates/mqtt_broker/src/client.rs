use shipyard_common::error::AppResult;
use rumqttc::{AsyncClient, EventLoop, MqttOptions};
use std::time::Duration;

/// MQTT client with auto-reconnect capabilities.
///
/// Full implementation in Milestone 1.4.
pub struct MqttClient {
    client: AsyncClient,
    eventloop: Option<EventLoop>,
}

impl MqttClient {
    /// Create a new MQTT client.
    pub fn new(host: &str, port: u16, client_id: &str) -> AppResult<Self> {
        let mut options = MqttOptions::new(client_id, host, port);
        options.set_keep_alive(Duration::from_secs(30));
        options.set_clean_session(true);

        let (client, eventloop) = AsyncClient::new(options, 256);

        tracing::info!("MQTT client created: {client_id}@{host}:{port}");

        Ok(Self {
            client,
            eventloop: Some(eventloop),
        })
    }

    /// Get the underlying async client for publishing.
    pub fn client(&self) -> &AsyncClient {
        &self.client
    }

    /// Take the event loop (can only be called once).
    pub fn take_eventloop(&mut self) -> Option<EventLoop> {
        self.eventloop.take()
    }
}
