use rumqttc::{AsyncClient, QoS};
use shipyard_common::error::{AppError, AppResult};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Manages MQTT subscriptions for a client connection.
///
/// Tracks active subscriptions to make `subscribe` idempotent — subscribing
/// to a topic that is already active is a no-op.
pub struct SubscriptionManager {
    client: AsyncClient,
    subscriptions: Arc<Mutex<HashSet<String>>>,
}

impl SubscriptionManager {
    /// Create a new `SubscriptionManager` wrapping the given `AsyncClient`.
    pub fn new(client: AsyncClient) -> Self {
        Self {
            client,
            subscriptions: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Subscribe to a topic with QoS 1. Idempotent — re-subscribing to the
    /// same topic string is a no-op.
    pub async fn subscribe(&self, topic: &str) -> AppResult<()> {
        let mut subs = self.subscriptions.lock().await;

        if subs.contains(topic) {
            tracing::debug!(topic, "already subscribed, skipping");
            return Ok(());
        }

        self.client
            .subscribe(topic, QoS::AtLeastOnce)
            .await
            .map_err(|e| AppError::Mqtt(format!("Subscribe error on '{topic}': {e}")))?;

        subs.insert(topic.to_string());
        tracing::info!(topic, "subscribed");

        Ok(())
    }

    /// Unsubscribe from a topic. If the topic is not currently tracked,
    /// the call still forwards the unsubscribe to the broker (so it works
    /// even after a reconnect that re-used a different `SubscriptionManager`).
    pub async fn unsubscribe(&self, topic: &str) -> AppResult<()> {
        self.client
            .unsubscribe(topic)
            .await
            .map_err(|e| AppError::Mqtt(format!("Unsubscribe error on '{topic}': {e}")))?;

        let mut subs = self.subscriptions.lock().await;
        subs.remove(topic);
        tracing::info!(topic, "unsubscribed");

        Ok(())
    }

    /// Subscribe to all topics for a service using a single wildcard:
    /// `platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/#`
    ///
    /// This covers status, containers, logs, and deployments in one shot.
    pub async fn subscribe_service(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
    ) -> AppResult<()> {
        let wildcard = format!(
            "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/#"
        );
        self.subscribe(&wildcard).await
    }

    /// Subscribe to all topics for a project using a single wildcard:
    /// `platform/orgs/{org_id}/projects/{project_id}/#`
    ///
    /// This covers the project topology topic and every service beneath it.
    pub async fn subscribe_project(&self, org_id: Uuid, project_id: Uuid) -> AppResult<()> {
        let wildcard =
            format!("platform/orgs/{org_id}/projects/{project_id}/#");
        self.subscribe(&wildcard).await
    }

    /// Unsubscribe from all topics for a service.
    ///
    /// Mirrors the wildcard used in `subscribe_service`.
    pub async fn unsubscribe_service(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
    ) -> AppResult<()> {
        let wildcard = format!(
            "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/#"
        );
        self.unsubscribe(&wildcard).await
    }

    /// Returns a snapshot of all currently active subscriptions.
    pub fn active_subscriptions(&self) -> Vec<String> {
        // We use `try_lock` here so the method can remain `&self` without
        // being async. In practice the lock is never held for long, so
        // contention is virtually impossible; fall back to an empty vec if
        // somehow the lock is held.
        match self.subscriptions.try_lock() {
            Ok(subs) => subs.iter().cloned().collect(),
            Err(_) => {
                tracing::warn!("active_subscriptions: lock contention, returning empty list");
                Vec::new()
            }
        }
    }
}
