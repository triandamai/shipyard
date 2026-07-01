//! MQTT Client Wrapper
//!
//! Wraps rumqttc with auto-reconnect, typed publishers, and subscription management.
//! All real-time events (logs, deployment status, container metrics) flow through MQTT.
//!
//! Full implementation in Milestone 1.4.

pub mod client;
pub mod event_loop;
pub mod publisher;
pub mod subscription;
pub mod topics;

pub use client::MqttClient;
pub use event_loop::spawn_event_loop;
pub use publisher::MqttPublisher;
pub use subscription::SubscriptionManager;

use rumqttc::{AsyncClient, MqttOptions};
use shipyard_common::error::AppResult;
use std::time::Duration;

/// Create a connected MQTT async client and split it into the three components
/// needed by the platform:
///
/// - [`MqttPublisher`] — publishes events (QoS 0 / QoS 1)
/// - [`SubscriptionManager`] — subscribes / unsubscribes to topics
/// - [`rumqttc::EventLoop`] — must be driven via [`spawn_event_loop`]
///
/// Both `MqttPublisher` and `SubscriptionManager` hold a clone of the same
/// underlying `AsyncClient`; rumqttc's `AsyncClient` is `Clone` and all
/// clones share the same send-channel.
pub fn create_mqtt_client(
    host: &str,
    port: u16,
    client_id: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> AppResult<(MqttPublisher, SubscriptionManager, rumqttc::EventLoop)> {
    let mut options = MqttOptions::new(client_id, host, port);
    options.set_keep_alive(Duration::from_secs(30));
    options.set_clean_session(true);

    if let (Some(user), Some(pass)) = (username, password) {
        options.set_credentials(user, pass);
        tracing::info!("MQTT client created with authentication: {client_id}@{host}:{port}");
    } else {
        tracing::warn!(
            "MQTT broker has no authentication configured — restrict port 1883 at the network level"
        );
    }

    let (async_client, eventloop) = AsyncClient::new(options, 256);

    tracing::info!("MQTT client created: {client_id}@{host}:{port}");

    let publisher = MqttPublisher::new(async_client.clone());
    let subscription_manager = SubscriptionManager::new(async_client);

    Ok((publisher, subscription_manager, eventloop))
}
