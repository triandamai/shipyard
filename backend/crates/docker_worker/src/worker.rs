use std::collections::HashMap;
use std::sync::Arc;

use bollard::system::EventsOptions;
use futures::StreamExt;
use shipyard_common::error::{AppError, AppResult};
use shipyard_common::types::MqttPayload;
use shipyard_docker::engine::DockerEngine;
use shipyard_mqtt::publisher::MqttPublisher;
use sqlx::PgPool;
use uuid::Uuid;

/// Docker Event Worker — subscribes to the Docker event stream, reconciles
/// container/service state into Postgres, and publishes real-time MQTT events.
///
/// Architecture:
/// ```text
/// Docker Socket → bollard::events() → handle_event() →
///     [container/service/network/volume/node handlers]
///         ↓                ↓
///     DB writes       MQTT publish
/// ```
pub struct DockerEventWorker {
    /// Raw bollard client for event streaming (trait doesn't expose streams).
    pub(crate) bollard: bollard::Docker,
    /// High-level Docker engine (for reconciliation helpers: list_services, list_tasks).
    pub(crate) engine: Arc<dyn DockerEngine>,
    /// Postgres connection pool.
    pub(crate) db: PgPool,
    /// MQTT publisher.
    pub(crate) mqtt: Arc<MqttPublisher>,
    /// Label prefix, e.g. `"platform"`.  Labels are expected as
    /// `{prefix}.service_id`, `{prefix}.managed`, etc.
    pub(crate) label_prefix: String,
}

impl DockerEventWorker {
    /// Construct a new worker.  Connects to the local Docker socket immediately.
    pub fn new(
        engine: Arc<dyn DockerEngine>,
        db: PgPool,
        mqtt: Arc<MqttPublisher>,
        label_prefix: impl Into<String>,
    ) -> AppResult<Self> {
        let bollard = bollard::Docker::connect_with_local_defaults()
            .map_err(|e| AppError::Docker(format!("Failed to connect to Docker socket: {e}")))?;
        Ok(Self {
            bollard,
            engine,
            db,
            mqtt,
            label_prefix: label_prefix.into(),
        })
    }

    // ─── Startup reconciliation ───────────────────────────────────────────────

    /// Walk all swarm services and tasks, upserting managed containers into
    /// Postgres so the DB reflects reality when the worker (re)starts.
    pub async fn reconcile_on_startup(&self) -> AppResult<()> {
        tracing::info!("Starting startup reconciliation");

        let services = self.engine.list_services().await?;
        tracing::info!("Found {} swarm services to reconcile", services.len());

        for svc in &services {
            let tasks = match self.engine.list_tasks(&svc.id).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(
                        service_id = %svc.id,
                        "Failed to list tasks during reconciliation: {e}"
                    );
                    continue;
                }
            };

            for task in &tasks {
                let container_id = match task.container_id.as_deref() {
                    Some(id) if !id.is_empty() => id,
                    _ => continue,
                };

                // Use labels from the task's ContainerSpec — these are set by
                // the deployment engine and are visible to the manager for ALL
                // tasks regardless of which node the container runs on.
                let service_id_key = format!("{}.service_id", self.label_prefix);
                let service_id_str = match task.labels.get(&service_id_key) {
                    Some(s) => s,
                    None => continue, // not managed by us
                };

                let service_id = match Uuid::parse_str(service_id_str) {
                    Ok(id) => id,
                    Err(_) => {
                        tracing::warn!(
                            label = %service_id_str,
                            "Invalid UUID in {}.service_id label, skipping",
                            self.label_prefix
                        );
                        continue;
                    }
                };

                let container_uuid = Uuid::now_v7();
                let replica_index = task.slot.map(|s| s as i32);
                let image = &task.image;
                let status = map_swarm_task_status(&task.status);
                let node_id = task.node_id.as_deref();
                let task_id = &task.id;

                sqlx::query(
                    r#"
                    INSERT INTO containers
                        (id, service_id, docker_container_id, docker_task_id, node_id,
                         replica_index, status, image, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7::container_status, $8, NOW(), NOW())
                    ON CONFLICT (docker_container_id)
                    DO UPDATE SET
                        status      = EXCLUDED.status,
                        node_id     = EXCLUDED.node_id,
                        updated_at  = NOW()
                    "#,
                )
                .bind(container_uuid)
                .bind(service_id)
                .bind(container_id)
                .bind(task_id)
                .bind(node_id)
                .bind(replica_index)
                .bind(status)
                .bind(image)
                .execute(&self.db)
                .await
                .ok(); // best-effort; row might not pass FK if service isn't in DB yet

                if let Err(e) = self.sync_service_replica_count(service_id).await {
                    tracing::warn!(
                        %service_id,
                        "Failed to sync replica count during reconciliation: {e}"
                    );
                }
            }
        }

        // ── Compose container reconciliation ──────────────────────────────
        // Walk every container in our DB that was inserted by compose sync and
        // check its live state against Docker.  This closes the gap where the
        // worker was restarted after a compose deploy and missed die/stop events.
        let compose_rows: Vec<(Uuid, String)> = sqlx::query_as::<_, (Uuid, String)>(
            r#"
            SELECT c.service_id, c.docker_container_id
            FROM containers c
            JOIN services s ON s.id = c.service_id
            WHERE s.type = 'docker_compose'
              AND c.docker_container_id IS NOT NULL
              AND c.docker_container_id != ''
              AND c.status NOT IN ('orphan', 'failed', 'shutdown', 'complete')
            "#,
        )
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        tracing::info!(
            "Reconciling {} compose containers on startup",
            compose_rows.len()
        );

        for (service_id, docker_id) in compose_rows {
            let detail = match self.engine.inspect_container(&docker_id).await {
                Ok(d) => d,
                Err(_) => {
                    // Container no longer exists in Docker → mark orphan
                    sqlx::query(
                        "UPDATE containers SET status = 'orphan'::container_status, updated_at = NOW() \
                         WHERE docker_container_id = $1",
                    )
                    .bind(&docker_id)
                    .execute(&self.db)
                    .await
                    .ok();
                    continue;
                }
            };

            let new_status = match detail.status.as_str() {
                "running" => "running",
                s if s.contains("exit") || s.contains("stop") || s == "dead" => {
                    if detail.exit_code == Some(0) { "shutdown" } else { "failed" }
                }
                _ => continue, // still starting up — leave as-is
            };

            sqlx::query(
                "UPDATE containers SET status = $1::container_status, updated_at = NOW() \
                 WHERE docker_container_id = $2",
            )
            .bind(new_status)
            .bind(&docker_id)
            .execute(&self.db)
            .await
            .ok();

            if let Err(e) = self.sync_service_replica_count(service_id).await {
                tracing::warn!(
                    %service_id,
                    docker_container_id = %docker_id,
                    "Failed to sync replica count during compose reconciliation: {e}"
                );
            }
        }

        tracing::info!("Startup reconciliation complete");
        Ok(())
    }

    // ─── Main event loop ──────────────────────────────────────────────────────

    /// Subscribe to the Docker event stream and process events indefinitely.
    ///
    /// Returns an error only if the stream itself fails.
    pub async fn run(&self) -> AppResult<()> {
        tracing::info!("Docker event worker starting");

        let mut filters: HashMap<&str, Vec<&str>> = HashMap::new();
        filters.insert(
            "type",
            vec!["container", "service", "network", "volume", "node"],
        );

        let mut stream = self.bollard.events(Some(EventsOptions::<&str> {
            filters,
            ..Default::default()
        }));

        tracing::info!("Subscribed to Docker event stream");

        while let Some(result) = stream.next().await {
            match result {
                Ok(event) => {
                    if let Err(e) = self.handle_event(event).await {
                        tracing::error!("Error handling Docker event: {e}");
                    }
                }
                Err(e) => {
                    tracing::error!("Docker event stream error: {e}");
                    return Err(AppError::Docker(e.to_string()));
                }
            }
        }

        tracing::warn!("Docker event stream ended unexpectedly");
        Ok(())
    }

    // ─── Event routing ────────────────────────────────────────────────────────

    async fn handle_event(&self, event: bollard::models::EventMessage) -> AppResult<()> {
        // Persist raw event for audit / debugging (best-effort).
        let raw = serde_json::to_value(&event).unwrap_or_default();
        let event_type = event
            .typ
            .as_ref()
            .map(|t| format!("{t}")) // Display impl already emits lowercase e.g. "container"
            .unwrap_or_default();
        let action = event.action.clone().unwrap_or_default();
        let actor_id = event
            .actor
            .as_ref()
            .and_then(|a| a.id.clone())
            .unwrap_or_default();
        let actor_attributes: Option<serde_json::Value> = event
            .actor
            .as_ref()
            .and_then(|a| a.attributes.as_ref())
            .map(|attrs| serde_json::to_value(attrs).unwrap_or_default());

        // Only persist events that carry meaningful state changes.
        // exec_* are health-check probes that fire every few seconds — skip them.
        let is_exec_noise = action.starts_with("exec_");
        if !is_exec_noise {
            sqlx::query(
                r#"
                INSERT INTO docker_events (event_type, action, actor_id, actor_attributes, raw)
                VALUES ($1, $2, $3, $4, $5)
                "#,
            )
            .bind(&event_type)
            .bind(&action)
            .bind(&actor_id)
            .bind(&actor_attributes)
            .bind(&raw)
            .execute(&self.db)
            .await
            .ok();
        }

        if is_exec_noise {
            tracing::trace!(event_type = %event_type, action = %action, actor_id = %actor_id, "Docker event");
        } else {
            tracing::debug!(event_type = %event_type, action = %action, actor_id = %actor_id, "Docker event");
        }

        // Route to the appropriate handler.
        let actor = event.actor.as_ref();
        match event_type.as_str() {
            "container" => self.handle_container_event(&action, actor).await?,
            "service" => self.handle_service_event(&action, actor).await?,
            "network" => self.handle_network_event(&action, actor).await?,
            "volume" => self.handle_volume_event(&action, actor).await?,
            "node" => self.handle_node_event(&action, actor).await?,
            other => {
                tracing::debug!("Ignoring unknown Docker event type: {other}");
            }
        }

        Ok(())
    }

    // ─── Container events ─────────────────────────────────────────────────────

    async fn handle_container_event(
        &self,
        action: &str,
        actor: Option<&bollard::models::EventActor>,
    ) -> AppResult<()> {
        let attrs = actor.and_then(|a| a.attributes.as_ref());
        let container_id = actor
            .and_then(|a| a.id.as_deref())
            .unwrap_or_default();

        // Only handle containers we created.
        let managed_key = format!("{}.managed", self.label_prefix);
        let is_managed = attrs
            .and_then(|a| a.get(&managed_key))
            .map(|v| v == "true")
            .unwrap_or(false);

        if !is_managed {
            // Not a Swarm-managed container. Check if we're tracking it as a
            // Docker Compose container (inserted by step_compose_sync_containers).
            return self
                .handle_compose_container_event(action, container_id, attrs)
                .await;
        }

        let service_id = match self.resolve_service_id(attrs) {
            Some(id) => id,
            None => {
                tracing::warn!(
                    container_id = %container_id,
                    action = %action,
                    "Managed container is missing {}.service_id label",
                    self.label_prefix
                );
                return Ok(());
            }
        };

        let exit_code: Option<i32> = attrs
            .and_then(|a| a.get("exitCode"))
            .and_then(|v| v.parse().ok());

        // Swarm adds these labels to every container it schedules.
        let task_id   = attrs.and_then(|a| a.get("com.docker.swarm.task.id")).cloned();
        let node_id   = attrs.and_then(|a| a.get("com.docker.swarm.node.id")).cloned();
        let image_val = attrs.and_then(|a| a.get("image")).cloned().unwrap_or_default();

        // Task name format: "<service_name>.<slot>.<task_hash>"
        // Parse the slot (replica index) from second-to-last dot-separated part.
        let replica_index: Option<i32> = attrs
            .and_then(|a| a.get("com.docker.swarm.task.name"))
            .and_then(|name| {
                let parts: Vec<&str> = name.split('.').collect();
                if parts.len() >= 2 {
                    parts[parts.len() - 2].parse::<i32>().ok()
                } else {
                    None
                }
            });

        match action {
            // Docker fires `create` before `start`. We INSERT here so `start`
            // (and any subsequent status changes) can find the row.
            "create" => {
                sqlx::query(
                    r#"
                    INSERT INTO containers
                        (id, service_id, docker_container_id, docker_task_id, node_id,
                         replica_index, status, image, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, 'pending'::container_status, $7, NOW(), NOW())
                    ON CONFLICT (docker_container_id)
                    DO UPDATE SET
                        status     = 'pending'::container_status,
                        updated_at = NOW()
                    "#,
                )
                .bind(Uuid::now_v7())
                .bind(service_id)
                .bind(container_id)
                .bind(task_id.as_deref())
                .bind(node_id.as_deref())
                .bind(replica_index)
                .bind(if image_val.is_empty() { None } else { Some(&image_val) })
                .execute(&self.db)
                .await
                .ok();
                tracing::info!(container_id, %service_id, ?replica_index, "Container created → inserted");
            }
            "start" => {
                // UPSERT so we recover if the `create` event was missed (e.g., worker restart).
                sqlx::query(
                    r#"
                    INSERT INTO containers
                        (id, service_id, docker_container_id, docker_task_id, node_id,
                         replica_index, status, image, started_at, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, 'running'::container_status, $7, NOW(), NOW(), NOW())
                    ON CONFLICT (docker_container_id)
                    DO UPDATE SET
                        status     = 'running'::container_status,
                        started_at = NOW(),
                        updated_at = NOW()
                    "#,
                )
                .bind(Uuid::now_v7())
                .bind(service_id)
                .bind(container_id)
                .bind(task_id.as_deref())
                .bind(node_id.as_deref())
                .bind(replica_index)
                .bind(if image_val.is_empty() { None } else { Some(&image_val) })
                .execute(&self.db)
                .await
                .ok();
                tracing::info!(container_id, %service_id, "Container started → running");
            }
            "die" | "kill" => {
                sqlx::query(
                    r#"
                    UPDATE containers
                    SET status      = 'failed'::container_status,
                        exit_code   = $1,
                        finished_at = NOW(),
                        updated_at  = NOW()
                    WHERE docker_container_id = $2
                    "#,
                )
                .bind(exit_code)
                .bind(container_id)
                .execute(&self.db)
                .await
                .ok();
            }
            "stop" => {
                sqlx::query(
                    r#"
                    UPDATE containers
                    SET status      = 'shutdown'::container_status,
                        finished_at = NOW(),
                        updated_at  = NOW()
                    WHERE docker_container_id = $1
                    "#,
                )
                .bind(container_id)
                .execute(&self.db)
                .await
                .ok();
            }
            "destroy" => {
                sqlx::query(
                    r#"
                    UPDATE containers
                    SET status     = 'orphan'::container_status,
                        updated_at = NOW()
                    WHERE docker_container_id = $1
                    "#,
                )
                .bind(container_id)
                .execute(&self.db)
                .await
                .ok();
            }
            other => {
                if !other.starts_with("exec_") {
                    tracing::debug!(container_id = %container_id, "Ignoring container action: {other}");
                }
                return Ok(());
            }
        }

        self.sync_service_replica_count(service_id).await?;
        self.publish_containers_update(service_id).await;
        Ok(())
    }

    // ─── Service events ───────────────────────────────────────────────────────

    async fn handle_service_event(
        &self,
        action: &str,
        actor: Option<&bollard::models::EventActor>,
    ) -> AppResult<()> {
        let service_docker_id = actor
            .and_then(|a| a.id.as_deref())
            .unwrap_or_default();
        let attrs = actor.and_then(|a| a.attributes.as_ref());

        match action {
            "remove" => {
                // Mark any containers belonging to this docker service as orphan,
                // then try to find our service_id to mark the service stopped.
                if let Some(service_id) = self.resolve_service_id(attrs) {
                    sqlx::query(
                        r#"
                        UPDATE services
                        SET status = 'stopped', updated_at = NOW()
                        WHERE id = $1
                        "#,
                    )
                    .bind(service_id)
                    .execute(&self.db)
                    .await
                    .ok();

                    tracing::info!(
                        %service_id,
                        docker_service_id = %service_docker_id,
                        "Service removed — marked stopped"
                    );

                    // Publish topology event so frontend refreshes.
                    if let Ok(Some((org_id, project_id))) = sqlx::query_as::<_, (Uuid, Uuid)>(
                        "SELECT p.org_id, s.project_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
                    )
                    .bind(service_id)
                    .fetch_optional(&self.db)
                    .await
                    {
                        let topic = shipyard_mqtt::topics::topology(org_id, project_id);
                        let payload = MqttPayload::new("service.deleted")
                            .with_meta(serde_json::json!({ "service_id": service_id }));
                        self.mqtt.publish_status(&topic, &payload).await.ok();
                    }
                } else {
                    // Service not managed by us, or label missing — warn only.
                    tracing::warn!(
                        docker_service_id = %service_docker_id,
                        "Received service remove event for untracked service"
                    );
                }
            }
            "update" => {
                // Detect external updates (not triggered by our platform).
                let managed_key = format!("{}.managed", self.label_prefix);
                let is_managed = attrs
                    .and_then(|a| a.get(&managed_key))
                    .map(|v| v == "true")
                    .unwrap_or(false);

                if !is_managed {
                    tracing::warn!(
                        docker_service_id = %service_docker_id,
                        "External service update detected — not managed by platform"
                    );
                }
            }
            other => {
                tracing::debug!(
                    docker_service_id = %service_docker_id,
                    "Received service event: {other}"
                );
            }
        }

        Ok(())
    }

    // ─── Network events ───────────────────────────────────────────────────────

    async fn handle_network_event(
        &self,
        action: &str,
        actor: Option<&bollard::models::EventActor>,
    ) -> AppResult<()> {
        let docker_network_id = actor
            .and_then(|a| a.id.as_deref())
            .unwrap_or_default();
        let network_name = actor
            .and_then(|a| a.attributes.as_ref())
            .and_then(|a| a.get("name"))
            .map(String::as_str)
            .unwrap_or_default();

        tracing::debug!(
            docker_network_id = %docker_network_id,
            network_name = %network_name,
            action = %action,
            "Docker network event"
        );

        if action == "destroy" {
            // Look up the DB row by Docker-assigned ID; if found, delete it and
            // publish a topology event so the frontend removes the node.
            let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid)>(
                r#"
                SELECT n.id, n.project_id, p.org_id
                FROM networks n
                JOIN projects p ON p.id = n.project_id
                WHERE n.docker_network_id = $1
                "#,
            )
            .bind(docker_network_id)
            .fetch_optional(&self.db)
            .await;

            if let Ok(Some((network_id, project_id, org_id))) = row {
                sqlx::query("DELETE FROM networks WHERE id = $1")
                    .bind(network_id)
                    .execute(&self.db)
                    .await
                    .ok();

                tracing::info!(
                    docker_network_id = %docker_network_id,
                    %network_id,
                    "Network destroyed externally — removed from DB"
                );

                let topic = shipyard_mqtt::topics::topology(org_id, project_id);
                let payload = MqttPayload::new("topology.changed").with_meta(serde_json::json!({
                    "resource":   "network",
                    "action":     "deleted",
                    "network_id": network_id,
                }));
                self.mqtt.publish_status(&topic, &payload).await.ok();
            }
        }

        Ok(())
    }

    // ─── Volume events ────────────────────────────────────────────────────────

    async fn handle_volume_event(
        &self,
        action: &str,
        actor: Option<&bollard::models::EventActor>,
    ) -> AppResult<()> {
        // Docker uses the volume name as the actor ID for volume events.
        let volume_name = actor
            .and_then(|a| a.id.as_deref())
            .unwrap_or_default();

        tracing::debug!(
            volume_name = %volume_name,
            action = %action,
            "Docker volume event"
        );

        if action == "destroy" {
            // Look up the DB row by volume name; if found, delete it and publish topology.
            let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid)>(
                r#"
                SELECT v.id, v.project_id, p.org_id
                FROM volumes v
                JOIN projects p ON p.id = v.project_id
                WHERE v.name = $1
                  AND v.project_id IS NOT NULL
                LIMIT 1
                "#,
            )
            .bind(volume_name)
            .fetch_optional(&self.db)
            .await;

            if let Ok(Some((volume_id, project_id, org_id))) = row {
                sqlx::query("DELETE FROM volumes WHERE id = $1")
                    .bind(volume_id)
                    .execute(&self.db)
                    .await
                    .ok();

                tracing::info!(
                    volume_name = %volume_name,
                    %volume_id,
                    "Volume destroyed externally — removed from DB"
                );

                let topic = shipyard_mqtt::topics::topology(org_id, project_id);
                let payload = MqttPayload::new("topology.changed").with_meta(serde_json::json!({
                    "resource":  "volume",
                    "action":    "deleted",
                    "volume_id": volume_id,
                }));
                self.mqtt.publish_status(&topic, &payload).await.ok();
            }
        }

        Ok(())
    }

    // ─── Node events ──────────────────────────────────────────────────────────

    async fn handle_node_event(
        &self,
        action: &str,
        actor: Option<&bollard::models::EventActor>,
    ) -> AppResult<()> {
        let node_id = actor
            .and_then(|a| a.id.as_deref())
            .unwrap_or_default();
        let attrs = actor.and_then(|a| a.attributes.as_ref());
        let node_hostname = attrs
            .and_then(|a| a.get("name"))
            .map(String::as_str)
            .unwrap_or_default();

        tracing::info!(
            node_id = %node_id,
            node_hostname = %node_hostname,
            action = %action,
            "Docker node event"
        );

        let payload = MqttPayload::new("node.event").with_meta(serde_json::json!({
            "action":        action,
            "node_id":       node_id,
            "node_hostname": node_hostname,
        }));

        let topic = shipyard_mqtt::topics::system_health();
        self.mqtt.publish_status(&topic, &payload).await.ok();

        Ok(())
    }

    // ─── Compose container events ─────────────────────────────────────────────

    /// Handle lifecycle events for containers created by `docker compose up`.
    /// These don't have our swarm labels, so we look them up by container ID.
    async fn handle_compose_container_event(
        &self,
        action: &str,
        container_id: &str,
        attrs: Option<&HashMap<String, String>>,
    ) -> AppResult<()> {
        match action {
            "start" | "die" | "kill" | "stop" | "destroy" => {}
            _ => return Ok(()),
        }

        // Find the service_id this container belongs to (may not exist if it
        // was never tracked by step_compose_sync_containers).
        let row = sqlx::query_as::<_, (Uuid,)>(
            "SELECT service_id FROM containers WHERE docker_container_id = $1",
        )
        .bind(container_id)
        .fetch_optional(&self.db)
        .await
        .ok()
        .flatten();

        let Some((service_id,)) = row else {
            return Ok(());
        };

        let exit_code: Option<i32> = attrs
            .and_then(|a| a.get("exitCode"))
            .and_then(|v| v.parse().ok());

        match action {
            "start" => {
                sqlx::query(
                    r#"
                    UPDATE containers
                    SET status      = 'running'::container_status,
                        started_at  = NOW(),
                        updated_at  = NOW()
                    WHERE docker_container_id = $1
                    "#,
                )
                .bind(container_id)
                .execute(&self.db)
                .await
                .ok();
                tracing::info!(container_id, %service_id, "Compose container started → running");
            }
            "die" | "kill" => {
                // Exit code 0 = graceful shutdown; non-zero = failed.
                let new_status = if exit_code == Some(0) { "shutdown" } else { "failed" };
                sqlx::query(
                    r#"
                    UPDATE containers
                    SET status      = $1::container_status,
                        exit_code   = $2,
                        finished_at = NOW(),
                        updated_at  = NOW()
                    WHERE docker_container_id = $3
                    "#,
                )
                .bind(new_status)
                .bind(exit_code)
                .bind(container_id)
                .execute(&self.db)
                .await
                .ok();
                tracing::info!(
                    container_id,
                    %service_id,
                    ?exit_code,
                    "Compose container died → {new_status}"
                );
            }
            "stop" => {
                sqlx::query(
                    r#"
                    UPDATE containers
                    SET status      = 'shutdown'::container_status,
                        finished_at = NOW(),
                        updated_at  = NOW()
                    WHERE docker_container_id = $1
                    "#,
                )
                .bind(container_id)
                .execute(&self.db)
                .await
                .ok();
                tracing::info!(container_id, %service_id, "Compose container stopped → shutdown");
            }
            "destroy" => {
                sqlx::query(
                    r#"
                    UPDATE containers
                    SET status     = 'orphan'::container_status,
                        updated_at = NOW()
                    WHERE docker_container_id = $1
                    "#,
                )
                .bind(container_id)
                .execute(&self.db)
                .await
                .ok();
                tracing::info!(
                    container_id,
                    %service_id,
                    "Compose container destroyed → orphan"
                );
            }
            _ => return Ok(()),
        }

        // Recount running containers → update service status → publish MQTT.
        self.sync_service_replica_count(service_id).await?;
        self.publish_containers_update(service_id).await;
        Ok(())
    }

    // ─── Helpers ──────────────────────────────────────────────────────────────

    /// Extract and parse the platform service UUID from container/service labels.
    fn resolve_service_id(
        &self,
        attrs: Option<&HashMap<String, String>>,
    ) -> Option<Uuid> {
        let key = format!("{}.service_id", self.label_prefix);
        let id_str = attrs?.get(&key)?;
        Uuid::parse_str(id_str).ok()
    }

    /// Publish a lightweight `topology.changed` signal so the frontend re-fetches
    /// the topology from the API (which now includes live container nodes).
    ///
    /// We deliberately do NOT embed container data in the payload — the broker
    /// has a 10 KB packet limit and a full container list can exceed that easily.
    async fn publish_containers_update(&self, service_id: Uuid) {
        let ids = sqlx::query_as::<_, (Uuid, Uuid)>(
            "SELECT p.org_id, s.project_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await;

        let (org_id, project_id) = match ids {
            Ok(Some(row)) => row,
            _ => return,
        };

        let topic = shipyard_mqtt::topics::topology(org_id, project_id);
        let payload = MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({ "service_id": service_id }));
        self.mqtt.publish_status(&topic, &payload).await.ok();
    }

    /// Recount running containers for `service_id`, update the services table,
    /// and publish MQTT events for replica count and service status.
    pub async fn sync_service_replica_count(&self, service_id: Uuid) -> AppResult<()> {
        // Count currently-running containers (live observed state).
        let row = sqlx::query_as::<_, (i64,)>(
            "SELECT COUNT(*) FROM containers WHERE service_id = $1 AND status = 'running'::container_status",
        )
        .bind(service_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(format!("Failed to count running containers: {e}")))?;

        let running = row.0 as i32;
        let svc_status = if running > 0 { "running" } else { "stopped" };

        // Update ONLY status — intentionally do NOT touch `replicas`.
        //
        // `services.replicas` is the user-configured desired count and must only
        // change when the user explicitly scales the service.  Writing the live
        // running count here caused a race with rolling updates (START_FIRST
        // strategy briefly creates N+1 containers) that incremented the desired
        // replica count on every webhook-triggered deployment.
        sqlx::query(
            "UPDATE services SET status = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(svc_status)
        .bind(service_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(format!("Failed to update service status: {e}")))?;

        // Fetch org_id and project_id to build the MQTT topic.
        let ids = sqlx::query_as::<_, (Uuid, Uuid)>(
            r#"
            SELECT p.org_id, s.project_id
            FROM services s
            JOIN projects p ON p.id = s.project_id
            WHERE s.id = $1
            "#,
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(format!("Failed to fetch org/project for service: {e}")))?;

        if let Some((org_id, project_id)) = ids {
            // Replica count topic.
            let replica_topic =
                shipyard_mqtt::topics::replicas_count(org_id, project_id, service_id);
            let replica_payload = MqttPayload::new("replica.count").with_meta(serde_json::json!({
                "running":    running,
                "service_id": service_id,
            }));
            self.mqtt
                .publish_status(&replica_topic, &replica_payload)
                .await
                .ok();

            // Service status topic.
            let status_topic =
                shipyard_mqtt::topics::service_status(org_id, project_id, service_id);
            let status_payload =
                MqttPayload::new("service.status").with_meta(serde_json::json!({
                    "status":   svc_status,
                    "replicas": running,
                }));
            self.mqtt
                .publish_status(&status_topic, &status_payload)
                .await
                .ok();
        }

        Ok(())
    }

    /// Poll all Swarm tasks and upsert container records into Postgres.
    ///
    /// Called periodically so that containers running on worker nodes (whose
    /// Docker events are invisible to the manager's local socket) stay in sync.
    /// Safe to call concurrently with the event worker — all writes are upserts.
    ///
    /// Also handles orphan detection: any DB container that was non-terminal but
    /// no longer appears in the Swarm task list gets marked "orphan" so the UI
    /// can show it as stopped and allow the user to delete the record.
    pub async fn sync_swarm_tasks(&self) -> AppResult<()> {
        let services = self.engine.list_services().await?;
        let mut affected: Vec<Uuid> = Vec::new();
        let service_id_key = format!("{}.service_id", self.label_prefix);

        for svc in &services {
            let tasks = match self.engine.list_tasks(&svc.id).await {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(service_id = %svc.id, "sync_swarm_tasks: list_tasks failed: {e}");
                    continue;
                }
            };

            // Track (service_uuid → set of docker_container_ids seen in Swarm) for
            // orphan detection below.
            let mut seen: std::collections::HashMap<Uuid, Vec<String>> =
                std::collections::HashMap::new();

            for task in &tasks {
                let container_id = match task.container_id.as_deref() {
                    Some(id) if !id.is_empty() => id,
                    _ => continue,
                };

                let service_id_str = match task.labels.get(&service_id_key) {
                    Some(s) => s,
                    None => continue,
                };

                let service_id = match Uuid::parse_str(service_id_str) {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                // If DesiredState is "shutdown" the Swarm has decided to stop the task —
                // map it terminal regardless of what the current Status.State shows.
                let status = if task.desired_state == "shutdown" {
                    "shutdown"
                } else {
                    map_swarm_task_status(&task.status)
                };

                let replica_index = task.slot.map(|s| s as i32);
                let node_id = task.node_id.as_deref();

                sqlx::query(
                    r#"
                    INSERT INTO containers
                        (id, service_id, docker_container_id, docker_task_id, node_id,
                         replica_index, status, image, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7::container_status, $8, NOW(), NOW())
                    ON CONFLICT (docker_container_id)
                    DO UPDATE SET
                        status     = EXCLUDED.status,
                        node_id    = EXCLUDED.node_id,
                        updated_at = NOW()
                    "#,
                )
                .bind(Uuid::now_v7())
                .bind(service_id)
                .bind(container_id)
                .bind(&task.id)
                .bind(node_id)
                .bind(replica_index)
                .bind(status)
                .bind(&task.image)
                .execute(&self.db)
                .await
                .ok();

                seen.entry(service_id).or_default().push(container_id.to_string());

                if !affected.contains(&service_id) {
                    affected.push(service_id);
                }
            }

            // ── Orphan detection ──────────────────────────────────────────────
            // For each service we processed above, any DB container that is still
            // in a non-terminal status but whose docker_container_id was NOT seen
            // in the current Swarm task list means the task was garbage-collected
            // by Docker (task-history-limit).  Mark those as "orphan" so the
            // replica panel shows them as stopped and allows deletion.
            for (service_id, active_ids) in &seen {
                // Build a Postgres ANY($1) array from the active container IDs.
                let orphaned: Vec<Uuid> = sqlx::query_as::<_, (Uuid,)>(
                    r#"
                    SELECT id FROM containers
                    WHERE service_id = $1
                      AND docker_container_id IS NOT NULL
                      AND docker_container_id != ''
                      AND status NOT IN (
                            'orphan'::container_status,
                            'failed'::container_status,
                            'shutdown'::container_status,
                            'complete'::container_status,
                            'rejected'::container_status
                          )
                      AND docker_container_id != ALL($2)
                    "#,
                )
                .bind(service_id)
                .bind(active_ids)
                .fetch_all(&self.db)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|(id,)| id)
                .collect();

                if !orphaned.is_empty() {
                    tracing::info!(
                        %service_id,
                        count = orphaned.len(),
                        "sync_swarm_tasks: marking {} container(s) as orphan (no longer in Swarm)",
                        orphaned.len()
                    );
                    sqlx::query(
                        r#"
                        UPDATE containers
                        SET status = 'orphan'::container_status, updated_at = NOW()
                        WHERE id = ANY($1)
                        "#,
                    )
                    .bind(&orphaned)
                    .execute(&self.db)
                    .await
                    .ok();
                }
            }
        }

        for service_id in affected {
            if let Err(e) = self.sync_service_replica_count(service_id).await {
                tracing::warn!(%service_id, "sync_swarm_tasks: replica count sync failed: {e}");
            }
        }

        tracing::debug!("sync_swarm_tasks complete");
        Ok(())
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Map a swarm task state string to our `container_status` enum value.
fn map_swarm_task_status(state: &str) -> &'static str {
    match state {
        "running" => "running",
        "complete" => "complete",
        "failed" | "rejected" => "failed",
        "shutdown" => "shutdown",
        "preparing" | "assigned" | "accepted" | "ready" | "starting" => "preparing",
        _ => "pending",
    }
}
