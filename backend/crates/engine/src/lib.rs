//! Core Deployment Engine
//!
//! Orchestrates the deployment pipeline for Shipyard services:
//!
//! Step 0 — Validate config
//! Step 1 — Pull image
//! Step 2 — Apply env vars
//! Step 3 — Configure volumes
//! Step 4 — Configure networks
//! Step 5 — Configure domains (Traefik labels)
//! Step 6 — Create/update swarm service
//! Step 7 — Mark deployment success/failed

use std::collections::HashMap;
use std::sync::Arc;

use shipyard_common::error::{AppError, AppResult};
use shipyard_common::types::{LogLevel, MqttPayload};
use shipyard_docker::engine::DockerEngine;
use shipyard_docker::types::{MountSpec, MountType, PortSpec, ServiceSpec};
use shipyard_git::GitService;
use shipyard_mqtt::publisher::MqttPublisher;
use shipyard_mqtt::topics;
use sqlx::PgPool;
use uuid::Uuid;

// ─── Image source ─────────────────────────────────────────────────────────────

/// Describes where the container image for this deployment comes from.
#[derive(Debug, Clone)]
enum ImageSource {
    /// Pull a pre-built image from a registry.
    Registry { image: String, tag: String },
    /// Clone / pull a git repo then run `docker build`.
    Git {
        repo_url: String,
        branch: String,
        /// Absolute filesystem path where the repo is (or will be) checked out.
        repo_path: String,
    },
}

// ─── Step metadata ────────────────────────────────────────────────────────────

const STEPS: [(i32, &str); 8] = [
    (0, "validate_config"),
    (1, "pull_image"),
    (2, "apply_env_vars"),
    (3, "configure_volumes"),
    (4, "configure_networks"),
    (5, "configure_domains"),
    (6, "create_or_update_service"),
    (7, "finalize"),
];

// ─── DeploymentEngine ────────────────────────────────────────────────────────

pub struct DeploymentEngine {
    pub docker: Arc<dyn DockerEngine>,
    pub db: PgPool,
    pub mqtt: Arc<MqttPublisher>,
    pub label_prefix: String,
    pub traefik_network: String,
    pub secret_key: String,
    /// Use a socat bridge-mode proxy container for port publishing instead of
    /// Swarm's native EndpointSpec. Set to true on macOS Docker Desktop;
    /// false on Linux where Swarm host-mode ports bind directly.
    pub port_proxy: bool,
}

impl DeploymentEngine {
    pub fn new(
        docker: Arc<dyn DockerEngine>,
        db: PgPool,
        mqtt: Arc<MqttPublisher>,
        label_prefix: String,
        traefik_network: String,
        secret_key: String,
        port_proxy: bool,
    ) -> Self {
        Self {
            docker,
            db,
            mqtt,
            label_prefix,
            traefik_network,
            secret_key,
            port_proxy,
        }
    }

    /// Entry point. Returns the deployment_id immediately.
    /// The caller should use this in a `tokio::spawn`.
    pub async fn deploy(
        &self,
        service_id: Uuid,
        triggered_by: &str,
        source_ref: &str,
    ) -> AppResult<Uuid> {
        // ── Look up the service and its project/org ──────────────────────────
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, i32, String)>(
            "SELECT s.id, p.org_id, s.project_id::text, p.id::text, s.replicas, s.type::text
             FROM services s
             JOIN projects p ON p.id = s.project_id
             WHERE s.id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let (_, org_id, project_id_str, _, _, svc_type) = row
            .ok_or_else(|| AppError::NotFound(format!("Service '{service_id}' not found")))?;

        let project_id: Uuid = project_id_str
            .parse()
            .map_err(|_| AppError::Internal("Invalid project_id UUID".to_string()))?;

        // ── Create the deployment record ─────────────────────────────────────
        let deployment_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
             VALUES ($1, $2, $3, $4, 'running', NOW())",
        )
        .bind(deployment_id)
        .bind(service_id)
        .bind(triggered_by)
        .bind(source_ref)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        self.execute_deployment(
            org_id, project_id, service_id, deployment_id,
            triggered_by, source_ref, &svc_type,
        )
        .await?;
        Ok(deployment_id)
    }

    /// Resume a deployment row that is already in the DB with status='queued'.
    /// Updates the row to 'running' then executes the same steps as `deploy`.
    pub async fn deploy_queued(
        &self,
        deployment_id: Uuid,
        service_id: Uuid,
        triggered_by: &str,
        source_ref: &str,
    ) -> AppResult<Uuid> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, i32, String)>(
            "SELECT s.id, p.org_id, s.project_id::text, p.id::text, s.replicas, s.type::text
             FROM services s
             JOIN projects p ON p.id = s.project_id
             WHERE s.id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let (_, org_id, project_id_str, _, _, svc_type) = row
            .ok_or_else(|| AppError::NotFound(format!("Service '{service_id}' not found")))?;

        let project_id: Uuid = project_id_str
            .parse()
            .map_err(|_| AppError::Internal("Invalid project_id UUID".to_string()))?;

        sqlx::query(
            "UPDATE deployments SET status = 'running'::deployment_status WHERE id = $1",
        )
        .bind(deployment_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        self.execute_deployment(
            org_id, project_id, service_id, deployment_id,
            triggered_by, source_ref, &svc_type,
        )
        .await?;
        Ok(deployment_id)
    }

    async fn execute_deployment(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        triggered_by: &str,
        source_ref: &str,
        svc_type: &str,
    ) -> AppResult<()> {
        // Publish overall deployment start
        let status_topic = topics::deployment_status(org_id, project_id, service_id, deployment_id);
        let _ = self
            .mqtt
            .publish_status(
                &status_topic,
                &MqttPayload::new("deployment.started")
                    .with_message(LogLevel::Info, "Deployment started")
                    .with_meta(serde_json::json!({
                        "deployment_id": deployment_id,
                        "service_id": service_id,
                        "triggered_by": triggered_by,
                        "source_ref": source_ref,
                    })),
            )
            .await;

        // ── Run all steps ────────────────────────────────────────────────────
        let result = if svc_type == "docker_compose" {
            self.run_compose_steps(org_id, project_id, service_id, deployment_id)
                .await
        } else {
            self.run_steps(
                org_id,
                project_id,
                service_id,
                deployment_id,
                triggered_by,
                source_ref,
            )
            .await
        };

        // ── Mark deployment final status ─────────────────────────────────────
        let (final_status, log_level, log_msg) = match &result {
            Ok(_) => ("success", "info", "Deployment completed successfully"),
            Err(e) => {
                tracing::error!("Deployment {deployment_id} failed: {e}");
                ("failed", "error", "Deployment failed")
            }
        };

        if let Err(e) = sqlx::query(
            "UPDATE deployments SET status = $1::deployment_status, finished_at = NOW() WHERE id = $2",
        )
        .bind(final_status)
        .bind(deployment_id)
        .execute(&self.db)
        .await
        {
            tracing::error!(
                deployment_id = %deployment_id,
                error = %e,
                "failed to persist deployment final status"
            );
        }

        let _ = self
            .mqtt
            .publish_status(
                &status_topic,
                &MqttPayload::new(format!("deployment.{final_status}"))
                    .with_message(
                        if log_level == "info" { LogLevel::Info } else { LogLevel::Error },
                        log_msg,
                    )
                    .with_meta(serde_json::json!({
                        "deployment_id": deployment_id,
                        "status": final_status,
                    })),
            )
            .await;

        let topology_topic = topics::topology(org_id, project_id);
        let _ = self
            .mqtt
            .publish_status(
                &topology_topic,
                &MqttPayload::new("topology.changed").with_meta(serde_json::json!({
                    "reason": "deployment",
                    "service_id": service_id,
                    "deployment_id": deployment_id,
                    "status": final_status,
                })),
            )
            .await;

        result?;
        Ok(())
    }

    /// Execute all 8 deployment steps in sequence.
    async fn run_steps(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        _triggered_by: &str,
        _source_ref: &str,
    ) -> AppResult<()> {
        // Pre-insert all steps with status 'pending'
        for (order_index, name) in &STEPS {
            sqlx::query(
                "INSERT INTO deployment_steps (id, deployment_id, name, status, order_index, started_at)
                 VALUES ($1, $2, $3, 'pending', $4, NULL)",
            )
            .bind(Uuid::new_v4())
            .bind(deployment_id)
            .bind(name)
            .bind(order_index)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        // Step 0: Validate config — resolves to either a registry image or a git source
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 0).await?;
        let validate_result = self.step_validate_config(service_id).await;
        let image_source = match self
            .finish_step(org_id, project_id, service_id, deployment_id, step_id, validate_result)
            .await?
        {
            Some(v) => v,
            None => return Err(AppError::Internal("validate_config returned no value".into())),
        };

        // Step 1: Acquire image — pull from registry OR clone+build from git
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 1).await?;
        let acquire_result = self.step_acquire_image(org_id, project_id, service_id, deployment_id, step_id, &image_source).await;
        let resolved_image_ref = match self
            .finish_step(org_id, project_id, service_id, deployment_id, step_id, acquire_result)
            .await?
        {
            Some(v) => v,
            None => return Err(AppError::Internal("acquire_image returned no value".into())),
        };

        // Step 2: Apply env vars — returns Vec<String> of "KEY=VALUE"
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 2).await?;
        let env_result = self.step_apply_env_vars(service_id).await;
        let env_vars = match self
            .finish_step(
                org_id,
                project_id,
                service_id,
                deployment_id,
                step_id,
                env_result,
            )
            .await?
        {
            Some(v) => v,
            None => return Err(AppError::Internal("apply_env_vars returned no value".into())),
        };

        // Step 3: Configure volumes — returns Vec<MountSpec>
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 3).await?;
        let vol_result = self.step_configure_volumes(service_id).await;
        let mounts = match self
            .finish_step(
                org_id,
                project_id,
                service_id,
                deployment_id,
                step_id,
                vol_result,
            )
            .await?
        {
            Some(v) => v,
            None => return Err(AppError::Internal("configure_volumes returned no value".into())),
        };

        // Step 4: Configure networks — returns Vec<String> of network names
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 4).await?;
        let net_result = self.step_configure_networks(service_id).await;
        let networks = match self
            .finish_step(
                org_id,
                project_id,
                service_id,
                deployment_id,
                step_id,
                net_result,
            )
            .await?
        {
            Some(v) => v,
            None => return Err(AppError::Internal("configure_networks returned no value".into())),
        };

        // Step 5: Configure domains — returns HashMap<String, String> of traefik labels
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 5).await?;
        let dom_result = self
            .step_configure_domains(service_id, project_id)
            .await;
        let traefik_labels = match self
            .finish_step(
                org_id,
                project_id,
                service_id,
                deployment_id,
                step_id,
                dom_result,
            )
            .await?
        {
            Some(v) => v,
            None => return Err(AppError::Internal("configure_domains returned no value".into())),
        };

        // Step 6: Create/update swarm service
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 6).await?;
        let svc_result = self
            .step_create_or_update_service(
                org_id,
                project_id,
                service_id,
                &resolved_image_ref,
                env_vars,
                mounts,
                networks,
                traefik_labels,
            )
            .await;
        self.finish_step(
            org_id,
            project_id,
            service_id,
            deployment_id,
            step_id,
            svc_result,
        )
        .await?;

        // Step 7: Finalize — update service status
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 7).await?;
        let fin_result = self.step_finalize(service_id).await;
        self.finish_step(
            org_id,
            project_id,
            service_id,
            deployment_id,
            step_id,
            fin_result,
        )
        .await?;

        Ok(())
    }

    // ── Step lifecycle helpers ────────────────────────────────────────────────

    /// Mark a step as 'running', publish a step status event, and return (step_id, order_index).
    async fn begin_step(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        order_index: i32,
    ) -> AppResult<(Uuid, i32)> {
        let row = sqlx::query_as::<_, (Uuid,)>(
            "UPDATE deployment_steps
             SET status = 'running', started_at = NOW()
             WHERE deployment_id = $1 AND order_index = $2
             RETURNING id",
        )
        .bind(deployment_id)
        .bind(order_index)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let step_id = row.0;
        let topic = topics::deployment_step_status(org_id, project_id, service_id, deployment_id, step_id);
        let _ = self.mqtt.publish_status(
            &topic,
            &MqttPayload::new("deployment.step.status").with_meta(serde_json::json!({
                "step_id": step_id,
                "order_index": order_index,
                "status": "running",
            })),
        ).await;

        Ok((step_id, order_index))
    }

    /// Finish a step: update its status, log the outcome, and propagate errors.
    ///
    /// If the step result is `Err`, the step is marked failed and the error is
    /// re-wrapped so the caller can bail the whole deployment.
    async fn finish_step<T>(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        step_id: Uuid,
        result: AppResult<T>,
    ) -> AppResult<Option<T>> {
        match result {
            Ok(val) => {
                sqlx::query(
                    "UPDATE deployment_steps
                     SET status = 'success', finished_at = NOW()
                     WHERE id = $1",
                )
                .bind(step_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

                self.insert_log(deployment_id, Some(step_id), "info", "Step completed successfully")
                    .await;
                self.publish_step_log(
                    org_id,
                    project_id,
                    service_id,
                    deployment_id,
                    step_id,
                    "info",
                    "Step completed successfully",
                )
                .await;
                let _ = self.mqtt.publish_status(
                    &topics::deployment_step_status(org_id, project_id, service_id, deployment_id, step_id),
                    &MqttPayload::new("deployment.step.status").with_meta(serde_json::json!({
                        "step_id": step_id,
                        "status": "success",
                    })),
                ).await;

                Ok(Some(val))
            }
            Err(e) => {
                let msg = e.to_string();

                sqlx::query(
                    "UPDATE deployment_steps
                     SET status = 'failed', finished_at = NOW()
                     WHERE id = $1",
                )
                .bind(step_id)
                .execute(&self.db)
                .await
                .ok();

                self.insert_log(deployment_id, Some(step_id), "error", &msg)
                    .await;
                self.publish_step_log(
                    org_id,
                    project_id,
                    service_id,
                    deployment_id,
                    step_id,
                    "error",
                    &msg,
                )
                .await;
                let _ = self.mqtt.publish_status(
                    &topics::deployment_step_status(org_id, project_id, service_id, deployment_id, step_id),
                    &MqttPayload::new("deployment.step.status").with_meta(serde_json::json!({
                        "step_id": step_id,
                        "status": "failed",
                    })),
                ).await;

                Err(e)
            }
        }
    }

    // ── MQTT helpers ──────────────────────────────────────────────────────────

    async fn publish_step_log(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        step_id: Uuid,
        level: &str,
        msg: &str,
    ) {
        let topic =
            topics::deployment_step_log(org_id, project_id, service_id, deployment_id, step_id);
        let log_level = match level {
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            "debug" => LogLevel::Debug,
            _ => LogLevel::Info,
        };
        let payload = MqttPayload::new("deployment.step.log")
            .with_message(log_level, msg)
            .with_meta(serde_json::json!({
                "step_id": step_id,
                "deployment_id": deployment_id,
            }));
        let _ = self.mqtt.publish_log(&topic, &payload).await;
    }

    // ── DB log helper ─────────────────────────────────────────────────────────

    async fn insert_log(
        &self,
        deployment_id: Uuid,
        step_id: Option<Uuid>,
        level: &str,
        message: &str,
    ) {
        let _ = sqlx::query(
            "INSERT INTO deployment_logs (id, deployment_id, step_id, level, message, timestamp)
             VALUES ($1, $2, $3, $4::log_level, $5, NOW())",
        )
        .bind(Uuid::new_v4())
        .bind(deployment_id)
        .bind(step_id)
        .bind(level)
        .bind(message)
        .execute(&self.db)
        .await;
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Docker Compose deploy path
    // ─────────────────────────────────────────────────────────────────────────

    const COMPOSE_STEPS: [(i32, &'static str); 4] = [
        (0, "validate_compose_file"),
        (1, "docker_compose_up"),
        (2, "sync_containers"),
        (3, "update_child_statuses"),
    ];

    const GIT_COMPOSE_STEPS: [(i32, &'static str); 6] = [
        (0, "validate_git_config"),
        (1, "clone_or_pull_repo"),
        (2, "sync_compose_children"),
        (3, "docker_compose_up"),
        (4, "sync_containers"),
        (5, "update_child_statuses"),
    ];

    /// Compose-specific deploy pipeline.
    /// Routes to the git-compose path when `git_repo_url` is set, otherwise uses
    /// the stored `docker-compose.yml` that was written at import time.
    async fn run_compose_steps(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
    ) -> AppResult<()> {
        let (directory_path, git_repo_url, git_branch): (String, Option<String>, String) =
            sqlx::query_as(
                "SELECT directory_path, git_repo_url, git_branch
                 FROM services WHERE id = $1",
            )
            .bind(service_id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

        if let Some(repo_url) = git_repo_url.filter(|u| !u.is_empty()) {
            self.run_git_compose_steps(
                org_id, project_id, service_id, deployment_id,
                directory_path, repo_url, git_branch,
            )
            .await
        } else {
            self.run_stored_compose_steps(
                org_id, project_id, service_id, deployment_id, directory_path,
            )
            .await
        }
    }

    /// Stored-compose path (YAML was written to disk at import time).
    async fn run_stored_compose_steps(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        directory_path: String,
    ) -> AppResult<()> {
        for (order_index, name) in &Self::COMPOSE_STEPS {
            sqlx::query(
                "INSERT INTO deployment_steps (id, deployment_id, name, status, order_index, started_at)
                 VALUES ($1, $2, $3, 'pending', $4, NULL)",
            )
            .bind(Uuid::new_v4())
            .bind(deployment_id)
            .bind(name)
            .bind(order_index)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        let compose_file = self.find_compose_file(&directory_path)?;

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 0).await?;
        let r = self.step_validate_compose_file(&compose_file).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 1).await?;
        let r = self.step_compose_up(
            org_id, project_id, service_id, deployment_id, step_id, &directory_path, &compose_file,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 2).await?;
        let r = self.step_compose_sync_containers(service_id, &directory_path, &compose_file).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 3).await?;
        let r = self.step_compose_finalize(org_id, project_id, service_id).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        Ok(())
    }

    /// Git-compose path: clone/pull the repo, sync DB children from the compose
    /// file in the repo, then run `docker compose up -d`.
    async fn run_git_compose_steps(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        directory_path: String,
        repo_url: String,
        git_branch: String,
    ) -> AppResult<()> {
        for (order_index, name) in &Self::GIT_COMPOSE_STEPS {
            sqlx::query(
                "INSERT INTO deployment_steps (id, deployment_id, name, status, order_index, started_at)
                 VALUES ($1, $2, $3, 'pending', $4, NULL)",
            )
            .bind(Uuid::new_v4())
            .bind(deployment_id)
            .bind(name)
            .bind(order_index)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        // Step 0: validate git config
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 0).await?;
        let r = self.step_validate_git_compose(&repo_url, &directory_path).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 1: clone or pull repo into directory_path
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 1).await?;
        let r = self.step_clone_or_pull(
            org_id, project_id, service_id, deployment_id, step_id,
            &repo_url, &git_branch, &directory_path,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 2: parse compose file and sync child services in DB
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 2).await?;
        let r = self.step_sync_compose_children(
            org_id, project_id, service_id, deployment_id, step_id, &directory_path,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 3: docker compose up -d
        let compose_file = self.find_compose_file(&directory_path)?;
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 3).await?;
        let r = self.step_compose_up(
            org_id, project_id, service_id, deployment_id, step_id, &directory_path, &compose_file,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 4: sync containers created by compose into the DB
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 4).await?;
        let r = self.step_compose_sync_containers(service_id, &directory_path, &compose_file).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 5: mark root + children as running, emit topology update
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 5).await?;
        let r = self.step_compose_finalize(org_id, project_id, service_id).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        Ok(())
    }

    /// Find the compose file in a directory, checking common filenames.
    fn find_compose_file(&self, directory_path: &str) -> AppResult<String> {
        for name in &[
            "docker-compose.yml",
            "docker-compose.yaml",
            "compose.yml",
            "compose.yaml",
        ] {
            let path = format!("{directory_path}/{name}");
            if std::path::Path::new(&path).exists() {
                return Ok(path);
            }
        }
        Err(AppError::BadRequest(format!(
            "No compose file found in '{directory_path}'. \
             Expected docker-compose.yml, docker-compose.yaml, compose.yml, or compose.yaml."
        )))
    }

    /// Rewrites any overlay networks in the compose YAML to include `attachable: true`
    /// so that `docker compose up -d` (standalone containers) can join them in Swarm mode.
    /// Writes a `.shipyard-compose.yml` alongside the original when a change is needed,
    /// and returns the path of the file to actually pass to `docker compose -f`.
    async fn preprocess_compose_for_deploy(
        &self,
        directory_path: &str,
        compose_file: &str,
    ) -> AppResult<String> {
        let content = tokio::fs::read_to_string(compose_file).await.map_err(|e| {
            AppError::Internal(format!("Cannot read compose file: {e}"))
        })?;

        let mut doc: serde_yaml::Value =
            serde_yaml::from_str(&content).map_err(|e| AppError::BadRequest(format!("Invalid compose YAML: {e}")))?;

        let mut modified = false;

        if let Some(networks) = doc.get_mut("networks").and_then(|v| v.as_mapping_mut()) {
            for (_, net_val) in networks.iter_mut() {
                if let serde_yaml::Value::Mapping(ref mut map) = net_val {
                    let driver = map
                        .get(&serde_yaml::Value::String("driver".to_string()))
                        .and_then(|v| v.as_str())
                        .unwrap_or("bridge");

                    if driver == "overlay" {
                        let already = map
                            .get(&serde_yaml::Value::String("attachable".to_string()))
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);

                        if !already {
                            map.insert(
                                serde_yaml::Value::String("attachable".to_string()),
                                serde_yaml::Value::Bool(true),
                            );
                            modified = true;
                        }
                    }
                }
            }
        }

        if !modified {
            return Ok(compose_file.to_string());
        }

        let new_content = serde_yaml::to_string(&doc)
            .map_err(|e| AppError::Internal(format!("Failed to serialize preprocessed compose: {e}")))?;

        let out = format!("{directory_path}/.shipyard-compose.yml");
        tokio::fs::write(&out, new_content).await.map_err(|e| {
            AppError::Internal(format!("Failed to write preprocessed compose: {e}"))
        })?;

        tracing::info!("Preprocessed compose: added attachable:true to overlay networks → {out}");
        Ok(out)
    }

    async fn step_validate_git_compose(&self, repo_url: &str, directory_path: &str) -> AppResult<()> {
        if repo_url.is_empty() {
            return Err(AppError::BadRequest(
                "Git repo URL is not set on this service.".to_string(),
            ));
        }
        // Pre-create the checkout directory so clone has a place to go.
        tokio::fs::create_dir_all(directory_path).await.map_err(|e| {
            AppError::Internal(format!("Failed to create checkout directory: {e}"))
        })?;
        // Check git CLI is available.
        let check = tokio::process::Command::new("git")
            .arg("--version")
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("'git' not found: {e}")))?;
        if !check.status.success() {
            return Err(AppError::Internal("'git' command failed".to_string()));
        }
        tracing::info!("Git compose validated: repo={repo_url} dir={directory_path}");
        Ok(())
    }

    /// Parse the compose file in `directory_path` and create/update child services in the DB.
    /// Existing children (matched by slug) are updated in-place; new ones are inserted.
    async fn step_sync_compose_children(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        step_id: Uuid,
        directory_path: &str,
    ) -> AppResult<()> {
        let compose_file = self.find_compose_file(directory_path)?;
        let content = tokio::fs::read_to_string(&compose_file).await.map_err(|e| {
            AppError::Internal(format!("Failed to read compose file: {e}"))
        })?;

        let doc: serde_yaml::Value = serde_yaml::from_str(&content)
            .map_err(|e| AppError::BadRequest(format!("Invalid compose YAML: {e}")))?;

        let services = match doc.get("services").and_then(|v| v.as_mapping()) {
            Some(m) => m.clone(),
            None => {
                let msg = "Compose file has no 'services:' section — nothing to sync";
                self.insert_log(deployment_id, Some(step_id), "warn", msg).await;
                return Ok(());
            }
        };

        let mut created = 0usize;
        let mut updated = 0usize;

        for (name_val, svc_val) in &services {
            let name = match name_val.as_str() {
                Some(s) => s,
                None => continue,
            };
            let image = svc_val
                .get("image")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let slug: String = name
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect();

            // Does this child already exist?
            let existing: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM services WHERE service_parent_id = $1 AND slug = $2",
            )
            .bind(service_id)
            .bind(&slug)
            .fetch_optional(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

            let child_id = if let Some((id,)) = existing {
                sqlx::query(
                    "UPDATE services SET image = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(&image)
                .bind(id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
                updated += 1;
                id
            } else {
                let id = Uuid::new_v4();
                let child_dir = format!("{directory_path}/{id}/{slug}");
                let replicas = svc_val
                    .get("replicas")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as i32;

                sqlx::query(
                    r#"INSERT INTO services
                           (id, project_id, name, slug, type, image, directory_path, ports,
                            status, replicas, service_parent_id, created_at, updated_at)
                       VALUES ($1, $2, $3, $4, 'docker'::service_type, $5, $6, '[]'::jsonb,
                               'stopped', $7, $8, NOW(), NOW())"#,
                )
                .bind(id)
                .bind(project_id)
                .bind(name)
                .bind(&slug)
                .bind(&image)
                .bind(&child_dir)
                .bind(replicas)
                .bind(service_id)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::Database(format!("Failed to create child '{name}': {e}")))?;

                created += 1;
                id
            };

            // Sync environment variables for this child.
            let env_vars = self.extract_compose_envs(svc_val);
            for (key, value) in &env_vars {
                sqlx::query(
                    "INSERT INTO service_envs
                         (id, service_id, key, value_encrypted, is_secret, created_at)
                     VALUES ($1, $2, $3, $4, FALSE, NOW())
                     ON CONFLICT (service_id, key)
                     DO UPDATE SET value_encrypted = EXCLUDED.value_encrypted",
                )
                .bind(Uuid::new_v4())
                .bind(child_id)
                .bind(key)
                .bind(value)
                .execute(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;
            }

            // Link child to its compose networks in service_networks.
            let svc_net_names: Vec<String> = match svc_val.get("networks") {
                Some(serde_yaml::Value::Sequence(arr)) => arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect(),
                Some(serde_yaml::Value::Mapping(map)) => map
                    .keys()
                    .filter_map(|k| k.as_str())
                    .map(String::from)
                    .collect(),
                _ => {
                    // No explicit networks: query all non-external networks for this project
                    // that belong to this compose stack (share project_id).
                    sqlx::query_as::<_, (String,)>(
                        "SELECT name FROM networks WHERE project_id = $1 ORDER BY created_at ASC",
                    )
                    .bind(project_id)
                    .fetch_all(&self.db)
                    .await
                    .map_err(|e| AppError::Database(e.to_string()))?
                    .into_iter()
                    .map(|(n,)| n)
                    .collect()
                }
            };

            for net_name in &svc_net_names {
                let net_row: Option<(Uuid,)> = sqlx::query_as(
                    "SELECT id FROM networks WHERE project_id = $1 AND name = $2",
                )
                .bind(project_id)
                .bind(net_name)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

                if let Some((net_id,)) = net_row {
                    sqlx::query(
                        "INSERT INTO service_networks (service_id, network_id)
                         VALUES ($1, $2) ON CONFLICT DO NOTHING",
                    )
                    .bind(child_id)
                    .bind(net_id)
                    .execute(&self.db)
                    .await
                    .map_err(|e| AppError::Database(e.to_string()))?;
                }
            }

            let msg = format!(
                "{}: {} child '{}' (image: {})",
                if created > 0 && updated == 0 { "Created" } else { "Synced" },
                if existing.is_some() { "updated" } else { "created" },
                name,
                if image.is_empty() { "not set" } else { &image }
            );
            self.insert_log(deployment_id, Some(step_id), "info", &msg).await;
            self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &msg).await;
        }

        let summary = format!(
            "Compose sync complete — {created} created, {updated} updated"
        );
        self.insert_log(deployment_id, Some(step_id), "info", &summary).await;
        self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &summary).await;
        Ok(())
    }

    /// Extract environment variables from a compose service YAML node.
    /// Handles both list format (`- KEY=value`) and map format (`KEY: value`).
    fn extract_compose_envs(&self, svc: &serde_yaml::Value) -> Vec<(String, String)> {
        match svc.get("environment") {
            Some(serde_yaml::Value::Mapping(map)) => map
                .iter()
                .filter_map(|(k, v)| {
                    Some((
                        k.as_str()?.to_string(),
                        v.as_str().unwrap_or("").to_string(),
                    ))
                })
                .collect(),
            Some(serde_yaml::Value::Sequence(seq)) => seq
                .iter()
                .filter_map(|v| v.as_str())
                .filter_map(|s| {
                    let mut parts = s.splitn(2, '=');
                    Some((
                        parts.next()?.to_string(),
                        parts.next().unwrap_or("").to_string(),
                    ))
                })
                .collect(),
            _ => vec![],
        }
    }

    async fn step_validate_compose_file(&self, compose_file: &str) -> AppResult<()> {
        if !tokio::fs::metadata(compose_file).await.is_ok() {
            return Err(AppError::BadRequest(format!(
                "Compose file not found at '{compose_file}'. \
                 Re-import the stack to restore it."
            )));
        }
        // Verify `docker compose` CLI is available
        let check = tokio::process::Command::new("docker")
            .args(["compose", "version"])
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to run 'docker compose': {e}")))?;

        if !check.status.success() {
            return Err(AppError::Internal(
                "'docker compose' command failed — ensure Docker Compose v2 is installed".to_string(),
            ));
        }
        tracing::info!("Compose file validated: {compose_file}");
        Ok(())
    }

    async fn step_compose_up(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        step_id: Uuid,
        directory_path: &str,
        compose_file: &str,
    ) -> AppResult<()> {
        // Collect env vars from all child services and write .env so docker
        // compose can use them for variable substitution in the YAML.
        let children = sqlx::query_as::<_, (String, String)>(
            "SELECT e.key, e.value_encrypted
             FROM service_envs e
             JOIN services s ON s.id = e.service_id
             WHERE s.service_parent_id = $1
               AND e.key NOT LIKE '__\\_%' ESCAPE '\\'
             ORDER BY e.key ASC",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if !children.is_empty() {
            let env_content: String = children
                .into_iter()
                .map(|(k, v)| {
                    let plain = shipyard_common::crypto::decrypt_or_passthrough(&self.secret_key, &v);
                    format!("{k}={plain}\n")
                })
                .collect();

            tokio::fs::write(format!("{directory_path}/.env"), env_content)
                .await
                .map_err(|e| {
                    AppError::Internal(format!("Failed to write .env file: {e}"))
                })?;

            let msg = "Wrote child service env vars to .env for variable substitution";
            self.insert_log(deployment_id, Some(step_id), "info", msg).await;
            self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", msg).await;
        }

        // Preprocess: overlay networks need attachable:true for standalone compose containers.
        let effective_file = self
            .preprocess_compose_for_deploy(directory_path, compose_file)
            .await?;

        let msg = format!("Running: docker compose -f {effective_file} up -d --remove-orphans");
        self.insert_log(deployment_id, Some(step_id), "info", &msg).await;
        self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &msg).await;

        let output = tokio::process::Command::new("docker")
            .args(["compose", "-f", &effective_file, "up", "-d", "--remove-orphans"])
            .current_dir(directory_path)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to spawn docker compose: {e}")))?;

        // Stream stdout + stderr as deployment logs
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        for line in stdout.lines().chain(stderr.lines()) {
            if !line.trim().is_empty() {
                self.insert_log(deployment_id, Some(step_id), "info", line).await;
                self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", line).await;
            }
        }

        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            return Err(AppError::Internal(format!(
                "'docker compose up -d' exited with code {code}"
            )));
        }

        tracing::info!("docker compose up -d completed for service {service_id}");
        Ok(())
    }

    async fn step_compose_finalize(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        root_service_id: Uuid,
    ) -> AppResult<()> {
        // Mark the root service as running
        sqlx::query(
            "UPDATE services SET status = 'running', updated_at = NOW() WHERE id = $1",
        )
        .bind(root_service_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        // Mark all child services as running
        sqlx::query(
            "UPDATE services SET status = 'running', updated_at = NOW()
             WHERE service_parent_id = $1",
        )
        .bind(root_service_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!("Compose stack {root_service_id}: root + children marked running");

        // Notify the frontend to refresh the topology (new containers are now in the DB).
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({ "source": "compose_deploy" }));
        let _ = self.mqtt.publish_status(&topic, &payload).await;

        Ok(())
    }

    /// Query `docker compose ps` after a successful deploy and upsert the
    /// container rows into the DB so the topology shows live replica nodes.
    async fn step_compose_sync_containers(
        &self,
        root_service_id: Uuid,
        directory_path: &str,
        compose_file: &str,
    ) -> AppResult<()> {
        let output = tokio::process::Command::new("docker")
            .args(["compose", "-f", compose_file, "ps", "--format", "json"])
            .current_dir(directory_path)
            .output()
            .await
            .map_err(|e| AppError::Internal(format!("docker compose ps failed: {e}")))?;

        if !output.status.success() {
            // Non-fatal — containers may still be starting up.
            tracing::warn!("docker compose ps returned non-zero; skipping container sync");
            return Ok(());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let text = stdout.trim();
        if text.is_empty() {
            return Ok(());
        }

        // `docker compose ps --format json` outputs either a JSON array (newer
        // Compose versions) or one JSON object per line (NDJSON, older versions).
        let items: Vec<serde_json::Value> = if text.starts_with('[') {
            serde_json::from_str(text).unwrap_or_default()
        } else {
            text.lines()
                .filter_map(|l| serde_json::from_str(l.trim()).ok())
                .collect()
        };

        // Build a compose-service-name → child_service_id map.
        let children: Vec<(String, Uuid)> = sqlx::query_as::<_, (String, Uuid)>(
            "SELECT name, id FROM services WHERE service_parent_id = $1",
        )
        .bind(root_service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let name_to_id: HashMap<String, Uuid> = children.into_iter().collect();

        for item in &items {
            let svc_name = item.get("Service").and_then(|v| v.as_str()).unwrap_or("");
            let container_id = item
                .get("ID")
                .or_else(|| item.get("Id"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let state_str = item
                .get("State")
                .or_else(|| item.get("Status"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let image = item
                .get("Image")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if container_id.is_empty() {
                continue;
            }
            let Some(&child_svc_id) = name_to_id.get(svc_name) else {
                continue;
            };

            let db_status = compose_state_to_container_status(state_str);

            sqlx::query(
                "INSERT INTO containers
                     (id, service_id, docker_container_id, status, image, replica_index,
                      created_at, updated_at)
                 VALUES ($1, $2, $3, $4::container_status, $5, 0, NOW(), NOW())
                 ON CONFLICT (docker_container_id) DO UPDATE
                   SET status     = EXCLUDED.status::container_status,
                       service_id = EXCLUDED.service_id,
                       image      = EXCLUDED.image,
                       updated_at = NOW()",
            )
            .bind(Uuid::new_v4())
            .bind(child_svc_id)
            .bind(container_id)
            .bind(db_status)
            .bind(&image)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        tracing::info!("Synced {} container(s) for compose stack {root_service_id}", items.len());
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Individual Step implementations
    // ─────────────────────────────────────────────────────────────────────────

    /// Step 0: Validate config and resolve where the image comes from.
    ///
    /// For `git` services: validates `git_repo_url` is set; returns `ImageSource::Git`.
    /// For all other types: resolves the Docker image from `services.image` or the
    /// `__IMAGE__` env var; returns `ImageSource::Registry`.
    async fn step_validate_config(&self, service_id: Uuid) -> AppResult<ImageSource> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
            "SELECT type::text, COALESCE(image, ''), git_repo_url, git_branch, directory_path
             FROM services WHERE id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Service '{service_id}' not found")))?;

        let (svc_type, image_col, git_repo_url, git_branch, directory_path) = row;

        if svc_type == "git" {
            // ── Git service ──────────────────────────────────────────────────
            let repo_url = git_repo_url.filter(|u| !u.is_empty()).ok_or_else(|| {
                AppError::BadRequest(
                    "Git service has no repository URL configured. \
                     Set git_repo_url on the service.".to_string(),
                )
            })?;

            tracing::info!("Validated git service: repo={repo_url} branch={git_branch}");
            Ok(ImageSource::Git {
                repo_url,
                branch: git_branch,
                repo_path: directory_path,
            })
        } else {
            // ── Image-based service ──────────────────────────────────────────
            let image_full = if !image_col.is_empty() {
                image_col
            } else {
                let row = sqlx::query_as::<_, (String,)>(
                    "SELECT value_encrypted FROM service_envs \
                     WHERE service_id = $1 AND key = '__IMAGE__'",
                )
                .bind(service_id)
                .fetch_optional(&self.db)
                .await
                .map_err(|e| AppError::Database(e.to_string()))?;

                row.map(|(v,)| v)
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        AppError::BadRequest(
                            "Service has no image configured. \
                             Set the image field or add a __IMAGE__ env var.".to_string(),
                        )
                    })?
            };

            // Split "image:tag" or default to "latest"
            let (image, tag) = if let Some(pos) = image_full.rfind(':') {
                (image_full[..pos].to_string(), image_full[pos + 1..].to_string())
            } else {
                (image_full, "latest".to_string())
            };

            tracing::info!("Validated config: image={image}:{tag}");
            Ok(ImageSource::Registry { image, tag })
        }
    }

    /// Step 1: Acquire the container image.
    ///
    /// Returns the resolved image reference to use in the service spec.
    /// For `Registry` sources: pulls from Docker Hub / registry, returns `"image:tag"`.
    /// For `Git` sources: clones/pulls the repo, builds with `docker build`, returns
    ///   `"shipyard/{slug}:{short_sha}"`.
    async fn step_acquire_image(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        step_id: Uuid,
        source: &ImageSource,
    ) -> AppResult<String> {
        match source {
            ImageSource::Registry { image, tag } => {
                tracing::info!("Pulling image: {image}:{tag}");
                let lines = self.docker.pull_image(image, tag).await?;
                for line in &lines {
                    self.insert_log(deployment_id, Some(step_id), "info", line).await;
                }
                Ok(format!("{image}:{tag}"))
            }

            ImageSource::Git { repo_url, branch, repo_path, .. } => {
                self.step_clone_or_pull(
                    org_id, project_id, service_id, deployment_id, step_id,
                    repo_url, branch, repo_path,
                ).await?;

                // Resolve HEAD commit to finalize the image tag
                let commit_info = self.git_head_commit(repo_path).await?;
                let sha_len = 7.min(commit_info.sha.len());
                let short_sha = &commit_info.sha[..sha_len];

                let slug = std::path::Path::new(repo_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("service")
                    .to_string();
                let local_tag = format!("shipyard/{slug}:{short_sha}");

                let msg = format!("Building image {local_tag} from commit {}", commit_info.sha);
                self.insert_log(deployment_id, Some(step_id), "info", &msg).await;
                self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &msg).await;

                let dockerfile_path = format!("{repo_path}/Dockerfile");
                let dockerfile = if std::path::Path::new(&dockerfile_path).exists() {
                    "Dockerfile"
                } else {
                    return Err(AppError::BadRequest(
                        "No Dockerfile found in repository root. \
                         Add a Dockerfile to the repo to enable git-based deployment.".to_string(),
                    ));
                };

                let build_lines = self.docker.build_image(&local_tag, repo_path, dockerfile).await?;

                for line in &build_lines {
                    if !line.trim().is_empty() {
                        self.insert_log(deployment_id, Some(step_id), "info", line).await;
                    }
                }

                // Store the built image tag so the UI can display what's deployed.
                sqlx::query("UPDATE services SET image = $1 WHERE id = $2")
                    .bind(&local_tag)
                    .bind(service_id)
                    .execute(&self.db)
                    .await
                    .map_err(|e| AppError::Database(e.to_string()))?;

                tracing::info!("Git build complete: {local_tag}");
                Ok(local_tag)
            }
        }
    }

    /// Clone the repo if it doesn't exist yet, otherwise pull latest changes.
    async fn step_clone_or_pull(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        step_id: Uuid,
        repo_url: &str,
        branch: &str,
        repo_path: &str,
    ) -> AppResult<()> {
        let repo_path = repo_path.to_string();
        let repo_url = repo_url.to_string();
        let branch = branch.to_string();

        let path_exists = tokio::fs::metadata(&repo_path).await.is_ok();

        if path_exists {
            let msg = format!("Pulling latest changes on branch '{branch}'");
            self.insert_log(deployment_id, Some(step_id), "info", &msg).await;
            self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &msg).await;

            tokio::task::spawn_blocking(move || {
                let git = GitService::new(&repo_path);
                git.pull_repo(&repo_path, Some(&branch))
            })
            .await
            .map_err(|e| AppError::Internal(format!("spawn_blocking panicked: {e}")))?
            .map_err(|e| AppError::Git(format!("pull failed: {e}")))?;
        } else {
            // Ensure parent directory exists
            if let Some(parent) = std::path::Path::new(&repo_path).parent() {
                tokio::fs::create_dir_all(parent).await
                    .map_err(|e| AppError::Internal(format!("mkdir failed: {e}")))?;
            }

            let msg = format!("Cloning {repo_url} (branch: {branch})");
            self.insert_log(deployment_id, Some(step_id), "info", &msg).await;
            self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &msg).await;

            let deployment_id_cp = deployment_id;
            let step_id_cp = step_id;
            let db = self.db.clone();
            let path_cp = repo_path.clone();

            tokio::task::spawn_blocking(move || {
                let git = GitService::new(&path_cp);
                git.clone_repo(&repo_url, &path_cp, Some(&branch), move |progress| {
                    // Fire-and-forget progress log (can't await inside blocking)
                    let db2 = db.clone();
                    let msg = progress.to_string();
                    tokio::spawn(async move {
                        let _ = sqlx::query(
                            "INSERT INTO deployment_logs (id, deployment_id, step_id, level, message, timestamp)
                             VALUES ($1, $2, $3, 'info'::log_level, $4, NOW())",
                        )
                        .bind(uuid::Uuid::new_v4())
                        .bind(deployment_id_cp)
                        .bind(step_id_cp)
                        .bind(&msg)
                        .execute(&db2)
                        .await;
                    });
                })
            })
            .await
            .map_err(|e| AppError::Internal(format!("spawn_blocking panicked: {e}")))?
            .map_err(|e| AppError::Git(format!("clone failed: {e}")))?;
        }

        Ok(())
    }

    /// Get the HEAD commit of a repository (blocking, called via spawn_blocking).
    async fn git_head_commit(&self, repo_path: &str) -> AppResult<shipyard_git::CommitInfo> {
        let path = repo_path.to_string();
        tokio::task::spawn_blocking(move || {
            let git = GitService::new(&path);
            git.head_commit(&path)
        })
        .await
        .map_err(|e| AppError::Internal(format!("spawn_blocking panicked: {e}")))?
    }

    /// Step 2: Read non-secret env vars, return as `KEY=VALUE` strings.
    async fn step_apply_env_vars(&self, service_id: Uuid) -> AppResult<Vec<String>> {
        // Fetch ALL env vars (including secrets) — secrets are encrypted and
        // must be decrypted before being injected into the container spec.
        // Internal vars (prefixed `__`) are excluded — they're platform metadata.
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT key, value_encrypted
             FROM service_envs
             WHERE service_id = $1
               AND key NOT LIKE '__\\_%' ESCAPE '\\'
             ORDER BY key ASC",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let env: Vec<String> = rows
            .into_iter()
            .map(|(k, v)| {
                let plaintext = shipyard_common::crypto::decrypt_or_passthrough(&self.secret_key, &v);
                format!("{k}={plaintext}")
            })
            .collect();

        tracing::info!("Applied {} env vars (including secrets)", env.len());
        Ok(env)
    }

    /// Step 3: Build MountSpec list from the volumes table.
    async fn step_configure_volumes(&self, service_id: Uuid) -> AppResult<Vec<MountSpec>> {
        let rows = sqlx::query_as::<_, (String, String, String)>(
            "SELECT name, mount_path, driver
             FROM volumes
             WHERE service_id = $1
             ORDER BY created_at ASC",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let mounts: Vec<MountSpec> = rows
            .into_iter()
            .map(|(name, mount_path, driver)| MountSpec {
                source: name,
                target: mount_path,
                mount_type: if driver == "local" || driver == "volume" {
                    MountType::Volume
                } else {
                    MountType::Bind
                },
                readonly: false,
            })
            .collect();

        tracing::info!("Configured {} mounts", mounts.len());
        Ok(mounts)
    }

    /// Step 4: Collect network names from service_networks + networks tables.
    /// Always includes the Traefik overlay network so Traefik can reach the
    /// service via its file-provider backend URL regardless of domain config.
    async fn step_configure_networks(&self, service_id: Uuid) -> AppResult<Vec<String>> {
        let rows = sqlx::query_as::<_, (String,)>(
            "SELECT n.name
             FROM service_networks sn
             JOIN networks n ON n.id = sn.network_id
             WHERE sn.service_id = $1
             ORDER BY n.name ASC",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let mut networks: Vec<String> = rows.into_iter().map(|(n,)| n).collect();

        // Always attach the Traefik overlay network so the service is reachable
        // by its DNS name from Traefik, even before domains are configured.
        if !networks.contains(&self.traefik_network) {
            networks.push(self.traefik_network.clone());
        }

        tracing::info!("Configured networks: {:?}", networks);
        Ok(networks)
    }

    /// Step 5: Build Traefik labels from the domains table.
    async fn step_configure_domains(
        &self,
        service_id: Uuid,
        _project_id: Uuid,
    ) -> AppResult<HashMap<String, String>> {
        let rows = sqlx::query_as::<_, (String, bool, String)>(
            "SELECT hostname, tls_enabled, traefik_router_name
             FROM domains
             WHERE service_id = $1
             ORDER BY created_at ASC",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let mut labels: HashMap<String, String> = HashMap::new();

        if rows.is_empty() {
            tracing::info!("No domains configured — skipping Traefik labels");
            return Ok(labels);
        }

        labels.insert("traefik.enable".to_string(), "true".to_string());

        for (hostname, tls_enabled, router_name) in &rows {
            // HTTP router
            labels.insert(
                format!("traefik.http.routers.{router_name}.rule"),
                format!("Host(`{hostname}`)"),
            );
            labels.insert(
                format!("traefik.http.routers.{router_name}.entrypoints"),
                "web".to_string(),
            );

            if *tls_enabled {
                // HTTPS router
                let tls_router = format!("{router_name}-tls");
                labels.insert(
                    format!("traefik.http.routers.{tls_router}.rule"),
                    format!("Host(`{hostname}`)"),
                );
                labels.insert(
                    format!("traefik.http.routers.{tls_router}.entrypoints"),
                    "websecure".to_string(),
                );
                labels.insert(
                    format!("traefik.http.routers.{tls_router}.tls"),
                    "true".to_string(),
                );
                labels.insert(
                    format!("traefik.http.routers.{tls_router}.tls.certresolver"),
                    "letsencrypt".to_string(),
                );
            }
        }

        tracing::info!("Configured {} domain(s), {} traefik labels", rows.len(), labels.len());
        Ok(labels)
    }

    /// Parse a port string ("3000", "8080:3000", "3000/udp", "8080:3000/tcp")
    /// into a `PortSpec`.
    fn parse_port_spec(s: &str) -> Option<PortSpec> {
        let s = s.trim();
        if s.is_empty() { return None; }
        let (mapping, proto) = if let Some(pos) = s.rfind('/') {
            (&s[..pos], s[pos + 1..].to_string())
        } else {
            (s, "tcp".to_string())
        };
        if let Some(pos) = mapping.find(':') {
            let published: u16 = mapping[..pos].trim().parse().ok()?;
            let target: u16 = mapping[pos + 1..].trim().parse().ok()?;
            Some(PortSpec { target, published: Some(published), protocol: proto })
        } else {
            let target: u16 = mapping.trim().parse().ok()?;
            Some(PortSpec { target, published: None, protocol: proto })
        }
    }

    /// Step 6: Create or update the Docker Swarm service.
    #[allow(clippy::too_many_arguments)]
    async fn step_create_or_update_service(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        image_ref: &str,
        env_vars: Vec<String>,
        mounts: Vec<MountSpec>,
        networks: Vec<String>,
        traefik_labels: HashMap<String, String>,
    ) -> AppResult<()> {
        let docker_svc_name = format!("{}-{}", self.label_prefix, service_id);

        // Read the intended replica count and port config from the DB.
        let svc_row = sqlx::query_as::<_, (i32, Option<serde_json::Value>)>(
            "SELECT replicas, ports FROM services WHERE id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let (db_replicas, db_ports_json) = svc_row.unwrap_or((1, None));
        let intended_replicas = (db_replicas.max(1)) as u64;

        // Parse port strings into PortSpec structs.
        let ports: Vec<PortSpec> = db_ports_json
            .as_ref()
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(Self::parse_port_spec)
                    .collect()
            })
            .unwrap_or_default();

        let port_summary = if ports.is_empty() {
            "none".to_string()
        } else {
            ports.iter().map(|p| {
                let host = p.published.map(|h| format!("{h}:")).unwrap_or_default();
                format!("{host}{}/{}", p.target, p.protocol)
            }).collect::<Vec<_>>().join(", ")
        };
        tracing::info!("Configured {} port(s): [{}]", ports.len(), port_summary);

        // Build labels: platform labels + traefik labels
        let mut labels: HashMap<String, String> = HashMap::new();
        labels.insert(
            format!("{}.managed", self.label_prefix),
            "true".to_string(),
        );
        labels.insert(
            format!("{}.service_id", self.label_prefix),
            service_id.to_string(),
        );
        labels.insert(
            format!("{}.project_id", self.label_prefix),
            project_id.to_string(),
        );
        labels.insert(
            format!("{}.org_id", self.label_prefix),
            org_id.to_string(),
        );

        // Merge traefik labels
        for (k, v) in traefik_labels {
            labels.insert(k, v);
        }

        // On macOS Docker Desktop (port_proxy=true) the socat proxy handles port
        // binding; Swarm must not publish any ports itself or it locks them via
        // iptables before the proxy container can bind.
        // On Linux (port_proxy=false) Swarm host-mode publishing works natively.
        let proxy_ports = if self.port_proxy { ports.clone() } else { vec![] };
        let swarm_ports = if self.port_proxy { vec![] } else { ports };

        let spec = ServiceSpec {
            name: docker_svc_name.clone(),
            image: image_ref.to_string(),
            replicas: intended_replicas,
            env: env_vars,
            labels,
            mounts,
            networks,
            ports: swarm_ports,
            resources: None,
        };

        // Upsert: try update first; create when the service doesn't exist yet.
        // This avoids the name-vs-ID mismatch that plagued the old list+check approach.
        let update_result = self.docker.update_service(&docker_svc_name, spec.clone()).await;
        match update_result {
            Ok(()) => {
                tracing::info!("Updated existing swarm service: {docker_svc_name}");
            }
            Err(e) if {
                let s = e.to_string().to_ascii_lowercase();
                s.contains("not found") || s.contains("no such service")
            } => {
                tracing::info!("Creating new swarm service: {docker_svc_name}");
                let svc_id = self.docker.create_service(spec).await?;
                tracing::info!("Created swarm service with Docker ID: {svc_id}");
            }
            Err(e) => return Err(e),
        }

        // Create bridge-mode socat proxy containers for each mapped port.
        // Swarm iptables-based port forwarding is not proxied by Docker Desktop
        // on macOS; standard docker-run bridge bindings are. The proxy connects
        // to the same overlay network so it can resolve the service by DNS name.
        self.manage_port_proxies(&docker_svc_name, &proxy_ports).await?;

        Ok(())
    }

    /// Create or replace socat proxy containers that bridge-bind each published port.
    ///
    /// Docker Desktop for macOS only proxies standard bridge-mode port bindings to
    /// macOS localhost; Swarm iptables rules are invisible to its proxy. We run one
    /// `alpine/socat` container per port in bridge mode (`-p host:container`) so
    /// Docker Desktop forwards the binding, and connect it to the same Swarm overlay
    /// so DNS resolution for the service name works inside the proxy container.
    async fn manage_port_proxies(
        &self,
        svc_name: &str,
        ports: &[PortSpec],
    ) -> AppResult<()> {
        if !self.port_proxy {
            return Ok(());
        }

        let published_ports: Vec<&PortSpec> = ports.iter().filter(|p| p.published.is_some()).collect();
        if published_ports.is_empty() {
            return Ok(());
        }

        // Pull socat image; ignore error if already present or if offline
        let _ = self.docker.pull_image("alpine/socat", "latest").await;

        for p in published_ports {
            let host_port = p.published.unwrap();
            let container_port = p.target;
            let proxy_name = format!("proxy-{svc_name}-{host_port}");

            // Force-remove existing container for this port (404 = fine, we ignore errors)
            let delete_path = format!("/v1.45/containers/{proxy_name}?force=true");
            let _ = shipyard_docker::raw_request("DELETE", &delete_path, None).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // PortBindings: container port → host port
            let mut port_bindings = serde_json::Map::new();
            port_bindings.insert(
                format!("{host_port}/tcp"),
                serde_json::json!([{"HostIp": "0.0.0.0", "HostPort": host_port.to_string()}]),
            );

            // Attach proxy to the same overlay so it can resolve the service by name
            let mut endpoints_config = serde_json::Map::new();
            endpoints_config.insert(self.traefik_network.clone(), serde_json::json!({}));

            let mut exposed_ports = serde_json::Map::new();
            exposed_ports.insert(format!("{host_port}/tcp"), serde_json::json!({}));

            let body = serde_json::json!({
                "Image": "alpine/socat:latest",
                "Cmd": [
                    format!("TCP-LISTEN:{host_port},fork,reuseaddr"),
                    format!("TCP:{svc_name}:{container_port}")
                ],
                "ExposedPorts": serde_json::Value::Object(exposed_ports),
                "HostConfig": {
                    "PortBindings": serde_json::Value::Object(port_bindings),
                    "RestartPolicy": {"Name": "unless-stopped"}
                },
                "NetworkingConfig": {
                    "EndpointsConfig": serde_json::Value::Object(endpoints_config)
                }
            });

            let create_path = format!("/v1.45/containers/create?name={proxy_name}");
            let resp = shipyard_docker::raw_request("POST", &create_path, Some(&body.to_string()))
                .await
                .map_err(|e| AppError::Docker(format!("create proxy {proxy_name}: {e}")))?;

            let v: serde_json::Value = serde_json::from_str(&resp)
                .map_err(|e| AppError::Docker(format!("parse proxy create response: {e}")))?;
            let container_id = v["Id"].as_str().unwrap_or(&proxy_name).to_string();

            let start_path = format!("/v1.45/containers/{container_id}/start");
            shipyard_docker::raw_request("POST", &start_path, None)
                .await
                .map_err(|e| AppError::Docker(format!("start proxy {proxy_name}: {e}")))?;

            tracing::info!(
                "Port proxy started: *:{host_port} -> {svc_name}:{container_port} [{proxy_name}]"
            );
        }

        Ok(())
    }

    /// Step 7: Finalize — mark the service as 'running' (preserves replicas set by user).
    async fn step_finalize(&self, service_id: Uuid) -> AppResult<()> {
        sqlx::query(
            "UPDATE services SET status = 'running', updated_at = NOW() WHERE id = $1",
        )
        .bind(service_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        tracing::info!("Service {service_id} marked as running");
        Ok(())
    }
}

/// Map a `docker compose ps` State/Status string to a `container_status` enum value.
fn compose_state_to_container_status(state: &str) -> &'static str {
    let lower = state.to_lowercase();
    if lower.contains("running") {
        "running"
    } else if lower.contains("exit") || lower.contains("stopped") {
        "shutdown"
    } else if lower.contains("dead") || lower.contains("failed") {
        "failed"
    } else if lower.contains("paused") {
        "shutdown"
    } else {
        "pending"
    }
}

