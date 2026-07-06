//! Core Deployment Engine

pub mod static_site;

use std::collections::HashMap;
use std::sync::Arc;

use shipyard_common::error::{AppError, AppResult};
use shipyard_common::types::{LogLevel, MqttPayload};
use shipyard_docker::engine::DockerEngine;
use shipyard_docker::types::{MountSpec, MountType, PortSpec, ResourceSpec, ServiceSpec};
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
    /// Absolute path to the data directory (e.g. /opt/shipyard/data).
    /// Static site files are stored at `{data_dir}/static/<service_id>/`.
    pub data_dir: String,
    /// How many past deploy versions to keep per static site.
    pub static_retention: usize,
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
        data_dir: String,
        static_retention: usize,
    ) -> Self {
        Self {
            docker,
            db,
            mqtt,
            label_prefix,
            traefik_network,
            secret_key,
            port_proxy,
            data_dir,
            static_retention,
        }
    }

    /// Entry point. Caller pre-inserts the deployment row and passes the ID so
    /// the API can return it immediately without a race-condition sleep/retry.
    pub async fn deploy(
        &self,
        deployment_id: Uuid,
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

        self.execute_deployment(
            org_id, project_id, service_id, deployment_id,
            triggered_by, source_ref, &svc_type,
        )
        .await?;
        Ok(deployment_id)
    }

    /// Rollback to a specific image reference (previously recorded in `deployed_image`).
    /// Skips validate+acquire steps and uses `image_ref` directly.
    pub async fn rollback(
        &self,
        deployment_id: Uuid,
        service_id: Uuid,
        triggered_by: &str,
        image_ref: &str,
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

        let (_, org_id, project_id_str, _, _, _) = row
            .ok_or_else(|| AppError::NotFound(format!("Service '{service_id}' not found")))?;

        let project_id: Uuid = project_id_str
            .parse()
            .map_err(|_| AppError::Internal("Invalid project_id UUID".to_string()))?;

        self.execute_rollback(org_id, project_id, service_id, deployment_id, triggered_by, image_ref)
            .await?;
        Ok(deployment_id)
    }

    async fn execute_rollback(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        triggered_by: &str,
        image_ref: &str,
    ) -> AppResult<()> {
        let status_topic = topics::deployment_status(org_id, project_id, service_id, deployment_id);
        let _ = self.mqtt.publish_status(
            &status_topic,
            &MqttPayload::new("deployment.started")
                .with_message(LogLevel::Info, "Rollback started")
                .with_meta(serde_json::json!({
                    "deployment_id": deployment_id,
                    "service_id": service_id,
                    "triggered_by": triggered_by,
                    "image_ref": image_ref,
                })),
        ).await;

        // Pre-insert steps — steps 0+1 are immediately marked skipped.
        for (order_index, name) in &STEPS {
            sqlx::query(
                "INSERT INTO deployment_steps (id, deployment_id, name, status, order_index, started_at)
                 VALUES ($1, $2, $3, 'pending', $4, NULL)",
            )
            .bind(Uuid::now_v7())
            .bind(deployment_id)
            .bind(name)
            .bind(order_index)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        // Skip step 0 (validate) and step 1 (pull_image) — mark them skipped.
        for idx in [0i32, 1i32] {
            sqlx::query(
                "UPDATE deployment_steps
                 SET status = 'skipped', started_at = NOW(), finished_at = NOW()
                 WHERE deployment_id = $1 AND order_index = $2",
            )
            .bind(deployment_id)
            .bind(idx)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        let msg = format!("Rolling back to pinned image: {image_ref}");
        self.insert_log(deployment_id, None, "info", &msg).await;

        // Record the image being rolled back to.
        let _ = sqlx::query("UPDATE deployments SET deployed_image = $1 WHERE id = $2")
            .bind(image_ref)
            .bind(deployment_id)
            .execute(&self.db)
            .await;

        // Steps 2–7: identical to a normal deployment.
        let result = self.run_steps_from_image(
            org_id, project_id, service_id, deployment_id, image_ref,
        ).await;

        let (final_status, log_level, log_msg) = match &result {
            Ok(_) => ("success", "info", "Rollback completed successfully"),
            Err(e) => {
                tracing::error!("Rollback {deployment_id} failed: {e}");
                ("failed", "error", "Rollback failed")
            }
        };

        let _ = sqlx::query(
            "UPDATE deployments SET status = $1::deployment_status, finished_at = NOW() WHERE id = $2",
        )
        .bind(final_status)
        .bind(deployment_id)
        .execute(&self.db)
        .await;

        let _ = self.mqtt.publish_status(
            &status_topic,
            &MqttPayload::new(format!("deployment.{final_status}"))
                .with_message(
                    if log_level == "info" { LogLevel::Info } else { LogLevel::Error },
                    log_msg,
                )
                .with_meta(serde_json::json!({ "deployment_id": deployment_id, "status": final_status })),
        ).await;

        result?;
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Static site deploy pipeline
    // ─────────────────────────────────────────────────────────────────────────

    const STATIC_UPLOAD_STEPS: [(i32, &'static str); 6] = [
        (0, "extract_archive"),
        (1, "parse_shipyard_config"),
        (2, "publish_files"),
        (3, "write_nginx_conf"),
        (4, "go_live"),
        (5, "finalize"),
    ];

    const STATIC_GIT_STEPS: [(i32, &'static str); 7] = [
        (0, "clone_or_pull"),
        (1, "build_site"),
        (2, "parse_shipyard_config"),
        (3, "publish_files"),
        (4, "write_nginx_conf"),
        (5, "go_live"),
        (6, "finalize"),
    ];

    async fn run_static_steps(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        _triggered_by: &str,
        source_ref: &str,
    ) -> AppResult<()> {
        // source_ref == "upload" means the artifact path is in deployment metadata;
        // any other value is treated as git-based.
        if source_ref == "upload" {
            self.run_static_upload_steps(org_id, project_id, service_id, deployment_id).await
        } else {
            self.run_static_git_steps(org_id, project_id, service_id, deployment_id).await
        }
    }

    /// Upload pipeline: archive already on disk, just extract → publish → nginx.
    async fn run_static_upload_steps(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
    ) -> AppResult<()> {
        // Steps were pre-inserted by the upload API endpoint.
        // Resolve artifact path from the MQTT meta we stored in deployment row.
        let artifact_path: String = sqlx::query_scalar(
            "SELECT source_ref FROM deployments WHERE id = $1",
        )
        .bind(deployment_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .unwrap_or_default();

        // Artifact is stored at: {sites_dir}/uploads/{service_id}/{deployment_id}.zip
        let sites_base = format!("{}/static", self.data_dir);
        let extract_dir = format!("{sites_base}/{service_id}/extracts/{deployment_id}");
        let version_dir = format!("{sites_base}/{service_id}/{deployment_id}");
        let public_dir  = format!("{version_dir}/public");
        let current_link = format!("{sites_base}/{service_id}/current");
        let conf_dir    = format!("{sites_base}/conf.d");
        let uploads_dir = format!("{sites_base}/uploads/{service_id}");
        let artifact = format!("{uploads_dir}/{deployment_id}.zip");

        // Ensure the nginx container exists and is running before we write configs or serve files.
        self.ensure_static_nginx(&sites_base).await?;

        // Step 0: extract_archive + validate static output
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 0).await?;
        let extract_dir_clone = extract_dir.clone();
        let r = self.static_step_extract(&artifact, &extract_dir, deployment_id, step_id).await
            .and_then(|_| {
                // Validate the extracted content is a proper static site
                crate::static_site::validate_static_output(std::path::Path::new(&extract_dir_clone))
            });
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 1: parse shipyard.json from extracted root (optional — defaults if absent)
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 1).await?;
        let deploy_config_result = tokio::task::spawn_blocking({
            let d = extract_dir.clone();
            move || crate::static_site::parse_shipyard_config(std::path::Path::new(&d))
        }).await.map_err(|e| AppError::Internal(format!("spawn_blocking: {e}")))?;
        let deploy_config = match self.finish_step(org_id, project_id, service_id, deployment_id, step_id, deploy_config_result).await? {
            Some(c) => c,
            None => return Err(AppError::Internal("parse_shipyard_config returned nothing".into())),
        };

        // Step 2: publish_files (copy extract_dir → version_dir/public, atomic swap)
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 2).await?;
        let r = self.static_step_publish(&extract_dir, &public_dir, deployment_id, step_id).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Clean up extract dir — files have been published; no need to keep the intermediate copy.
        let _ = tokio::fs::remove_dir_all(&extract_dir).await;

        // Step 3: write_nginx_conf — use the `current` symlink path so the config stays valid
        // across deployments and can be regenerated when domains change without knowing deployment_id.
        let current_public = format!("{sites_base}/{service_id}/current/public");
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 3).await?;
        let r = self.static_step_write_nginx_conf(
            service_id, &current_public, &sites_base, &conf_dir, &deploy_config, deployment_id, step_id,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 4: go_live — atomic symlink swap
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 4).await?;
        let r = self.static_step_go_live(
            &current_link, &version_dir,
            format!("{sites_base}/{service_id}"), self.static_retention,
            deployment_id, step_id,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 5: finalize
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 5).await?;
        let r = self.static_step_finalize(service_id, &deploy_config, deployment_id, step_id).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Clean up the artifact zip after successful deploy
        let _ = tokio::fs::remove_file(&artifact).await;

        Ok(())
    }

    /// Git pipeline: clone/pull → build → publish → nginx.
    async fn run_static_git_steps(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
    ) -> AppResult<()> {
        // Insert steps — upload path pre-inserts them; git path does it here.
        for (order_index, name) in &Self::STATIC_GIT_STEPS {
            sqlx::query(
                "INSERT INTO deployment_steps
                    (id, deployment_id, name, status, order_index, started_at)
                 VALUES ($1, $2, $3, 'pending', $4, NULL)
                 ON CONFLICT DO NOTHING",
            )
            .bind(Uuid::now_v7())
            .bind(deployment_id)
            .bind(name)
            .bind(order_index)
            .execute(&self.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        }

        // Load static config for this service (stored defaults)
        let cfg = sqlx::query_as::<_, (String, String, String, String, String, String)>(
            "SELECT source, build_command, output_dir, node_version, install_command, framework
             FROM static_site_configs WHERE service_id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let (_, stored_build_cmd, stored_output_dir, stored_node_ver, stored_install_cmd, _) =
            cfg.unwrap_or_else(|| (
                "git".into(), "bun run build".into(), "dist".into(),
                "1".into(), "bun install".into(), "custom".into(),
            ));

        let sites_base    = format!("{}/static", self.data_dir);
        let repo_path     = format!("{sites_base}/{service_id}/repo");
        let version_dir   = format!("{sites_base}/{service_id}/{deployment_id}");
        let public_dir    = format!("{version_dir}/public");
        let current_link  = format!("{sites_base}/{service_id}/current");
        let conf_dir      = format!("{sites_base}/conf.d");

        // Ensure the nginx container exists and is running before we write configs or serve files.
        self.ensure_static_nginx(&sites_base).await?;

        // Load git config from service row
        let git_row = sqlx::query_as::<_, (Option<String>, String, String)>(
            "SELECT git_repo_url, git_branch, directory_path FROM services WHERE id = $1",
        )
        .bind(service_id)
        .fetch_one(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let (git_repo_url, git_branch, _) = git_row;
        let repo_url = git_repo_url.filter(|u| !u.is_empty()).ok_or_else(|| {
            AppError::BadRequest("Static site has no git_repo_url configured".into())
        })?;

        // Step 0: clone_or_pull
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 0).await?;
        let r = self.step_clone_or_pull(
            org_id, project_id, service_id, deployment_id, step_id,
            &repo_url, &git_branch, &repo_path,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Determine build config — priority: shipyard.json > auto-detect > stored DB config.
        let (build_command, output_dir, node_version, install_command) = {
            let p = repo_path.clone();
            let (shipyard_cfg, detected) = tokio::task::spawn_blocking(move || {
                let dir = std::path::Path::new(&p);
                let cfg = crate::static_site::parse_shipyard_config(dir)?;
                let detected = if cfg.build.is_none() {
                    crate::static_site::detect_build_config(dir)
                } else {
                    None
                };
                Ok::<_, shipyard_common::error::AppError>((cfg, detected))
            }).await.map_err(|e| AppError::Internal(format!("spawn_blocking: {e}")))??;

            if let Some(ov) = &shipyard_cfg.build {
                self.insert_log(deployment_id, None, "info",
                    "Found shipyard.json — using its build overrides").await;
                let cmd = ov.command.as_deref().unwrap_or(&stored_build_cmd).to_string();
                let out = ov.output.as_deref().unwrap_or(&stored_output_dir).to_string();
                let nv  = ov.node_version.as_deref().unwrap_or(&stored_node_ver).to_string();
                let ic  = ov.install_command.as_deref().unwrap_or(&stored_install_cmd).to_string();
                (cmd, out, nv, ic)
            } else if let Some(det) = detected {
                self.insert_log(deployment_id, None, "info",
                    &format!("Auto-detected framework: {} — using defaults (build: '{}', output: '{}')",
                        det.framework, det.build_command, det.output_dir)).await;
                (det.build_command.to_string(), det.output_dir.to_string(),
                 det.node_version.to_string(), det.install_command.to_string())
            } else {
                self.insert_log(deployment_id, None, "info",
                    "No shipyard.json or recognised framework — using stored build config").await;
                (stored_build_cmd, stored_output_dir, stored_node_ver, stored_install_cmd)
            }
        };

        // Step 1: build_site — spawn ephemeral Docker container
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 1).await?;
        let built_output_path = {
            let build_result = self.static_step_build(
                &repo_path, &output_dir, &install_command, &build_command,
                &node_version, deployment_id, step_id, org_id, project_id, service_id,
            ).await;
            // Validate the output is a static site (not SSR/server)
            let validated = build_result.and_then(|p| {
                crate::static_site::validate_static_output(std::path::Path::new(&p))
                    .map(|_| p)
            });
            match self.finish_step(org_id, project_id, service_id, deployment_id, step_id, validated).await? {
                Some(p) => p,
                None => return Err(AppError::Internal("build_site returned nothing".into())),
            }
        };

        // Step 2: parse shipyard.json from repo root for runtime config (spa, headers, etc.)
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 2).await?;
        let deploy_config_result = tokio::task::spawn_blocking({
            let d = repo_path.clone();
            move || crate::static_site::parse_shipyard_config(std::path::Path::new(&d))
        }).await.map_err(|e| AppError::Internal(format!("spawn_blocking: {e}")))?;
        let deploy_config = match self.finish_step(org_id, project_id, service_id, deployment_id, step_id, deploy_config_result).await? {
            Some(c) => c,
            None => return Err(AppError::Internal("parse_shipyard_config returned nothing".into())),
        };

        // Step 3: publish_files
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 3).await?;
        let r = self.static_step_publish(&built_output_path, &public_dir, deployment_id, step_id).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Clean up the build output dir — contents are now in public_dir; keep the repo for incremental pulls.
        let _ = tokio::fs::remove_dir_all(&built_output_path).await;

        // Step 4: write_nginx_conf — use the `current` symlink path (see upload pipeline comment).
        let current_public = format!("{sites_base}/{service_id}/current/public");
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 4).await?;
        let r = self.static_step_write_nginx_conf(
            service_id, &current_public, &sites_base, &conf_dir, &deploy_config, deployment_id, step_id,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 5: go_live
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 5).await?;
        let r = self.static_step_go_live(
            &current_link, &version_dir,
            format!("{sites_base}/{service_id}"), self.static_retention,
            deployment_id, step_id,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        // Step 6: finalize
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 6).await?;
        let r = self.static_step_finalize(service_id, &deploy_config, deployment_id, step_id).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, r).await?;

        Ok(())
    }

    // ── Static step helpers ───────────────────────────────────────────────────

    async fn static_step_extract(
        &self,
        artifact: &str,
        extract_dir: &str,
        deployment_id: Uuid,
        step_id: Uuid,
    ) -> AppResult<()> {
        let artifact = artifact.to_string();
        let extract_dir = extract_dir.to_string();
        let msg = format!("Extracting archive to {extract_dir}");
        self.insert_log(deployment_id, Some(step_id), "info", &msg).await;

        tokio::task::spawn_blocking(move || {
            crate::static_site::extract_zip(
                std::path::Path::new(&artifact),
                std::path::Path::new(&extract_dir),
            )
        })
        .await
        .map_err(|e| AppError::Internal(format!("spawn_blocking panicked: {e}")))??;

        self.insert_log(deployment_id, Some(step_id), "info", "Archive extracted successfully").await;
        Ok(())
    }

    async fn static_step_publish(
        &self,
        src: &str,
        dst: &str,
        deployment_id: Uuid,
        step_id: Uuid,
    ) -> AppResult<()> {
        let src = src.to_string();
        let dst = dst.to_string();
        let msg = format!("Publishing files to {dst}");
        self.insert_log(deployment_id, Some(step_id), "info", &msg).await;

        let bytes = tokio::task::spawn_blocking(move || {
            crate::static_site::copy_dir_all(
                std::path::Path::new(&src),
                std::path::Path::new(&dst),
            )
        })
        .await
        .map_err(|e| AppError::Internal(format!("spawn_blocking panicked: {e}")))??;

        self.insert_log(
            deployment_id, Some(step_id), "info",
            &format!("Published {} bytes", bytes),
        ).await;
        Ok(())
    }

    /// Ensures the `shipyard-nginx-static` container exists and is running.
    ///
    /// First call (fresh install): pulls nginx:alpine and creates the container with
    /// the data dir bind-mounted at the same host path so nginx conf `root` directives
    /// match without translation.
    ///
    /// Subsequent calls: if the container exists and is running this is a no-op;
    /// if it's stopped it is started again.
    async fn ensure_static_nginx(&self, sites_base: &str) -> AppResult<()> {
        const CONTAINER: &str = "shipyard-nginx-static";

        // Ensure conf.d dir exists so the bind mount succeeds even before first deploy.
        let conf_dir = format!("{sites_base}/conf.d");
        tokio::fs::create_dir_all(&conf_dir).await
            .map_err(|e| AppError::Internal(format!("Cannot create conf.d dir: {e}")))?;

        // Write the custom error page and default server conf.
        // Always overwrite so the latest design is used after upgrades.
        let errors_dir = format!("{sites_base}/_errors");
        tokio::fs::create_dir_all(&errors_dir).await.ok();
        tokio::fs::write(format!("{errors_dir}/404.html"), crate::static_site::HTML_404.as_bytes()).await.ok();
        tokio::fs::write(
            format!("{conf_dir}/_default.conf"),
            crate::static_site::render_default_nginx_conf(sites_base).as_bytes(),
        ).await.ok();

        // Check if container exists: `docker inspect --format {{.State.Running}} <name>`
        let inspect = tokio::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Running}}", CONTAINER])
            .output().await;

        match inspect {
            Ok(o) if o.status.success() => {
                let running = String::from_utf8_lossy(&o.stdout).trim() == "true";
                if !running {
                    // Container exists but is stopped — start it.
                    let start = tokio::process::Command::new("docker")
                        .args(["start", CONTAINER])
                        .output().await
                        .map_err(|e| AppError::Internal(format!("docker start nginx: {e}")))?;
                    if !start.status.success() {
                        let err = String::from_utf8_lossy(&start.stderr);
                        return Err(AppError::Internal(format!("docker start nginx failed: {err}")));
                    }
                }
                Ok(())
            }
            _ => {
                // Container does not exist — create and start it.
                // We mount the entire sites_base at the same path inside the container so that
                // nginx `root` directives written by the engine (host paths) work as-is.
                // conf.d is also mounted to the nginx default include path.
                let traefik_network_label = format!("traefik.docker.network={}", self.traefik_network);
                let status = tokio::process::Command::new("docker")
                    .args([
                        "run", "-d",
                        "--name",    CONTAINER,
                        "--restart", "unless-stopped",
                        "--network", &self.traefik_network,
                        // Sites bind: host path → same path in container (no path translation needed)
                        "-v", &format!("{sites_base}:{sites_base}:ro"),
                        // conf.d bind: nginx reads *.conf from /etc/nginx/conf.d by default
                        "-v", &format!("{conf_dir}:/etc/nginx/conf.d:ro"),
                        // Traefik Docker-provider labels: catch-all for all unmatched domains.
                        // Priority 1 ensures explicit app routes always win.
                        "--label", "traefik.enable=true",
                        "--label", &traefik_network_label,
                        "--label", "traefik.http.routers.static-sites-http.rule=HostRegexp(`.+`)",
                        "--label", "traefik.http.routers.static-sites-http.priority=1",
                        "--label", "traefik.http.routers.static-sites-http.entrypoints=web",
                        "--label", "traefik.http.routers.static-sites-http.service=static-sites",
                        "--label", "traefik.http.routers.static-sites-https.rule=HostRegexp(`.+`)",
                        "--label", "traefik.http.routers.static-sites-https.priority=1",
                        "--label", "traefik.http.routers.static-sites-https.entrypoints=websecure",
                        "--label", "traefik.http.routers.static-sites-https.tls=true",
                        "--label", "traefik.http.routers.static-sites-https.service=static-sites",
                        "--label", "traefik.http.services.static-sites.loadbalancer.server.port=80",
                        "nginx:alpine",
                    ])
                    .status().await
                    .map_err(|e| AppError::Internal(format!("docker run nginx: {e}")))?;

                if !status.success() {
                    return Err(AppError::Internal(
                        "Failed to create shipyard-nginx-static container".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }

    async fn static_step_write_nginx_conf(
        &self,
        service_id: Uuid,
        serve_root: &str,
        sites_base: &str,
        conf_dir: &str,
        deploy_config: &crate::static_site::DeployConfig,
        deployment_id: Uuid,
        step_id: Uuid,
    ) -> AppResult<()> {
        // Fetch domains for this service
        let domains: Vec<String> = sqlx::query_scalar::<_, String>(
            "SELECT hostname FROM domains WHERE service_id = $1 ORDER BY created_at ASC",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let conf = crate::static_site::render_nginx_site_conf(
            &service_id.to_string(),
            &domains,
            serve_root,
            sites_base,
            deploy_config,
        );

        tokio::fs::create_dir_all(conf_dir).await
            .map_err(|e| AppError::Internal(format!("Cannot create conf.d dir: {e}")))?;

        let conf_path = format!("{conf_dir}/{service_id}.conf");
        tokio::fs::write(&conf_path, &conf).await
            .map_err(|e| AppError::Internal(format!("Cannot write nginx conf: {e}")))?;

        self.insert_log(
            deployment_id, Some(step_id), "info",
            &format!("Wrote nginx config to {conf_path} ({} domain(s))", domains.len()),
        ).await;

        // Reload nginx inside the managed container so the new server block takes effect.
        // Falls back gracefully if Docker isn't available in the current environment.
        let reload = tokio::process::Command::new("docker")
            .args(["exec", "shipyard-nginx-static", "nginx", "-s", "reload"])
            .output().await;
        match reload {
            Ok(o) if o.status.success() => {
                self.insert_log(deployment_id, Some(step_id), "info", "nginx reloaded").await;
            }
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                self.insert_log(
                    deployment_id, Some(step_id), "warn",
                    &format!("nginx reload returned non-zero (config written, may need manual reload): {stderr}"),
                ).await;
            }
            Err(e) => {
                self.insert_log(
                    deployment_id, Some(step_id), "warn",
                    &format!("nginx reload skipped (docker not in PATH): {e}"),
                ).await;
            }
        }

        Ok(())
    }

    async fn static_step_go_live(
        &self,
        current_link: &str,
        version_dir: &str,
        versions_root: String,
        retention: usize,
        deployment_id: Uuid,
        step_id: Uuid,
    ) -> AppResult<()> {
        let current = current_link.to_string();
        let version = version_dir.to_string();

        tokio::task::spawn_blocking(move || {
            crate::static_site::atomic_swap_symlink(
                std::path::Path::new(&current),
                std::path::Path::new(&version),
            )
        })
        .await
        .map_err(|e| AppError::Internal(format!("spawn_blocking panicked: {e}")))??;

        self.insert_log(deployment_id, Some(step_id), "info", "Symlink swapped — site is live").await;

        // Prune old versions (non-fatal)
        let versions_root_clone = versions_root;
        let pruned = tokio::task::spawn_blocking(move || {
            crate::static_site::prune_old_versions(
                std::path::Path::new(&versions_root_clone),
                retention,
            )
        })
        .await
        .ok()
        .and_then(|r| r.ok())
        .unwrap_or(0);

        if pruned > 0 {
            self.insert_log(
                deployment_id, Some(step_id), "info",
                &format!("Pruned {pruned} old version(s)"),
            ).await;
        }

        Ok(())
    }

    async fn static_step_finalize(
        &self,
        service_id: Uuid,
        deploy_config: &crate::static_site::DeployConfig,
        _deployment_id: Uuid,
        _step_id: Uuid,
    ) -> AppResult<()> {
        // Persist the runtime deploy_config into static_site_configs for future reference.
        let config_json = serde_json::to_value(deploy_config)
            .unwrap_or(serde_json::Value::Null);
        sqlx::query(
            "UPDATE static_site_configs
             SET deploy_config = $1, updated_at = NOW()
             WHERE service_id = $2",
        )
        .bind(config_json)
        .bind(service_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        sqlx::query(
            "UPDATE services SET status = 'running', updated_at = NOW() WHERE id = $1",
        )
        .bind(service_id)
        .execute(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        Ok(())
    }

    /// Build a static site inside an ephemeral Docker container.
    /// Returns the path to the built output directory inside `repo_path`.
    #[allow(clippy::too_many_arguments)]
    async fn static_step_build(
        &self,
        repo_path: &str,
        output_dir: &str,
        install_command: &str,
        build_command: &str,
        node_version: &str,
        deployment_id: Uuid,
        step_id: Uuid,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
    ) -> AppResult<String> {
        // Use the official Bun image — Bun installs packages without needing Python/node-gyp,
        // which avoids native-addon compilation errors common with npm on Alpine.
        // node_version is kept in the DB for documentation but doesn't affect the image tag.
        let image = "oven/bun:1-alpine";
        let container_name = format!("shipyard-build-{deployment_id}");
        let repo_abs = std::fs::canonicalize(repo_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| repo_path.to_string());

        let run_script = format!(
            "set -ex && cd /src && {install_command} && {build_command}"
        );

        let msg = format!("Building with {image} (bun): {install_command} && {build_command}");
        self.insert_log(deployment_id, Some(step_id), "info", &msg).await;
        self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &msg).await;

        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;
        use tokio::sync::mpsc;

        let mut child = Command::new("docker")
            .args([
                "run", "--rm", "--name", &container_name,
                "-v", &format!("{repo_abs}:/src"),
                &image,
                "sh", "-c", &run_script,
            ])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to spawn build container: {e}")))?;

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let stdout_reader = child.stdout.take().map(BufReader::new);
        let stderr_reader = child.stderr.take().map(BufReader::new);

        let tx_out = tx.clone();
        let stdout_task = tokio::spawn(async move {
            if let Some(reader) = stdout_reader {
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx_out.send(line);
                }
            }
        });
        let stderr_task = tokio::spawn(async move {
            if let Some(reader) = stderr_reader {
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx.send(line);
                }
            }
        });

        while let Some(line) = rx.recv().await {
            let clean = strip_ansi(&line);
            if !clean.trim().is_empty() {
                self.insert_log(deployment_id, Some(step_id), "info", &clean).await;
                self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &clean).await;
            }
        }
        let _ = tokio::join!(stdout_task, stderr_task);

        let status = child.wait().await
            .map_err(|e| AppError::Internal(format!("Build container wait error: {e}")))?;
        if !status.success() {
            let code = status.code().unwrap_or(-1);
            return Err(AppError::Internal(format!("Build container exited with code {code}")));
        }

        let built_dir = format!("{repo_path}/{output_dir}");
        if !std::path::Path::new(&built_dir).exists() {
            return Err(AppError::Internal(format!(
                "Build completed but output_dir '{output_dir}' was not created (check build_command)"
            )));
        }

        self.insert_log(deployment_id, Some(step_id), "info", &format!("Build succeeded → {built_dir}")).await;
        Ok(built_dir)
    }

    /// Run deployment steps 2–7 with a pre-resolved image reference.
    /// Used by rollback (steps 0+1 already skipped).
    async fn run_steps_from_image(
        &self,
        org_id: Uuid,
        project_id: Uuid,
        service_id: Uuid,
        deployment_id: Uuid,
        image_ref: &str,
    ) -> AppResult<()> {
        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 2).await?;
        let env_result = self.step_apply_env_vars(service_id).await;
        let env_vars = match self.finish_step(org_id, project_id, service_id, deployment_id, step_id, env_result).await? {
            Some(v) => v,
            None => return Err(AppError::Internal("apply_env_vars returned no value".into())),
        };

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 3).await?;
        let vol_result = self.step_configure_volumes(service_id).await;
        let mounts = match self.finish_step(org_id, project_id, service_id, deployment_id, step_id, vol_result).await? {
            Some(v) => v,
            None => return Err(AppError::Internal("configure_volumes returned no value".into())),
        };

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 4).await?;
        let net_result = self.step_configure_networks(service_id).await;
        let networks = match self.finish_step(org_id, project_id, service_id, deployment_id, step_id, net_result).await? {
            Some(v) => v,
            None => return Err(AppError::Internal("configure_networks returned no value".into())),
        };

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 5).await?;
        let dom_result = self.step_configure_domains(service_id, project_id).await;
        let traefik_labels = match self.finish_step(org_id, project_id, service_id, deployment_id, step_id, dom_result).await? {
            Some(v) => v,
            None => return Err(AppError::Internal("configure_domains returned no value".into())),
        };

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 6).await?;
        let svc_result = self.step_create_or_update_service(
            org_id, project_id, service_id, image_ref, env_vars, mounts, networks, traefik_labels,
        ).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, svc_result).await?;

        let (step_id, _) = self.begin_step(org_id, project_id, service_id, deployment_id, 7).await?;
        let fin_result = self.step_finalize(service_id).await;
        self.finish_step(org_id, project_id, service_id, deployment_id, step_id, fin_result).await?;

        Ok(())
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
        } else if svc_type == "static" {
            self.run_static_steps(
                org_id, project_id, service_id, deployment_id,
                triggered_by, source_ref,
            )
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
            .bind(Uuid::now_v7())
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

        // Record the resolved image so rollback can re-use the exact artifact.
        let _ = sqlx::query("UPDATE deployments SET deployed_image = $1 WHERE id = $2")
            .bind(&resolved_image_ref)
            .bind(deployment_id)
            .execute(&self.db)
            .await;

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
        .bind(Uuid::now_v7())
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
            .bind(Uuid::now_v7())
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
            .bind(Uuid::now_v7())
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
                let id = Uuid::now_v7();
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
                .bind(Uuid::now_v7())
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
        // Collect env vars from the root service itself and all child services,
        // then write a .env file so docker compose can substitute ${VAR} in YAML.
        // Root service vars take lowest priority; child vars (deduplicated by key) win.
        let all_envs = sqlx::query_as::<_, (String, String)>(
            "SELECT e.key, e.value_encrypted
             FROM service_envs e
             JOIN services s ON s.id = e.service_id
             WHERE (s.id = $1 OR s.service_parent_id = $1)
               AND e.key NOT LIKE '__\\_%' ESCAPE '\\'
             ORDER BY (s.id = $1)::int ASC, e.key ASC",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        if !all_envs.is_empty() {
            // Deduplicate: last writer wins (child entries come after root entries due to ORDER BY).
            let mut env_map: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for (k, v) in all_envs {
                let plain = shipyard_common::crypto::decrypt_or_passthrough(&self.secret_key, &v);
                env_map.insert(k, plain);
            }
            let env_content: String = env_map
                .into_iter()
                .map(|(k, v)| format!("{k}={v}\n"))
                .collect();

            tokio::fs::write(format!("{directory_path}/.env"), env_content)
                .await
                .map_err(|e| {
                    AppError::Internal(format!("Failed to write .env file: {e}"))
                })?;

            let msg = "Wrote service env vars to .env for ${VAR} substitution in compose YAML";
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

        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio::process::Command;
        use tokio::sync::mpsc;

        let mut child = Command::new("docker")
            .args(["compose", "-f", &effective_file, "up", "-d", "--remove-orphans"])
            .current_dir(directory_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| AppError::Internal(format!("Failed to spawn docker compose: {e}")))?;

        // Read stdout and stderr concurrently — docker compose writes image pull
        // progress to stderr, so sequential reading would block on stdout first.
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        let stdout_reader = child.stdout.take().map(BufReader::new);
        let stderr_reader = child.stderr.take().map(BufReader::new);

        let tx_out = tx.clone();
        let stdout_task = tokio::spawn(async move {
            if let Some(reader) = stdout_reader {
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx_out.send(line);
                }
            }
        });

        let stderr_task = tokio::spawn(async move {
            if let Some(reader) = stderr_reader {
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx.send(line);
                }
            }
        });

        // Forward lines to deployment logs as they arrive from either stream.
        while let Some(line) = rx.recv().await {
            if !line.trim().is_empty() {
                // Strip ANSI escape codes (docker compose uses them for colour output).
                let clean: String = strip_ansi(&line);
                if !clean.trim().is_empty() {
                    self.insert_log(deployment_id, Some(step_id), "info", &clean).await;
                    self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &clean).await;
                }
            }
        }

        let _ = tokio::join!(stdout_task, stderr_task);

        let status = child
            .wait()
            .await
            .map_err(|e| AppError::Internal(format!("docker compose wait error: {e}")))?;

        if !status.success() {
            let code = status.code().unwrap_or(-1);
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
            .bind(Uuid::now_v7())
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
            } else if svc_type == "static" {
                // Static services default to nginx:alpine when no image is configured.
                "nginx:alpine".to_string()
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
    /// Load DOCKER_REGISTRY / DOCKER_USERNAME / DOCKER_PASSWORD for a service.
    async fn load_registry_credentials(&self, service_id: Uuid) -> (String, String, String) {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT key, value_encrypted FROM service_envs
             WHERE service_id = $1 AND key IN ('DOCKER_REGISTRY','DOCKER_USERNAME','DOCKER_PASSWORD')",
        )
        .bind(service_id)
        .fetch_all(&self.db)
        .await
        .unwrap_or_default();

        let mut registry = String::new();
        let mut username = String::new();
        let mut password = String::new();
        for (k, v) in rows {
            let plain = shipyard_common::crypto::decrypt_or_passthrough(&self.secret_key, &v);
            match k.as_str() {
                "DOCKER_REGISTRY" => registry = plain,
                "DOCKER_USERNAME" => username = plain,
                "DOCKER_PASSWORD" => password = plain,
                _ => {}
            }
        }
        (registry, username, password)
    }

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
                let (registry, username, password) = self.load_registry_credentials(service_id).await;
                let auth = if !username.is_empty() && !password.is_empty() {
                    let server = if registry.is_empty() { "https://index.docker.io/v1/".to_string() } else { registry.clone() };
                    tracing::info!("Using registry credentials for user '{username}' at '{server}'");
                    Some((username, password, server))
                } else {
                    None
                };
                let lines = self.docker.pull_image(image, tag, auth.as_ref().map(|(u, p, s)| (u.as_str(), p.as_str(), s.as_str()))).await?;
                for line in &lines {
                    self.insert_log(deployment_id, Some(step_id), "info", line).await;
                    self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", line).await;
                }
                // Resolve the pulled image to its digest so Docker Swarm sees
                // a real change on each redeploy (tag strings like `:latest` are
                // static and Swarm won't restart containers if the ref is unchanged).
                let resolved = self.docker.resolve_image_digest(image, tag).await?;
                tracing::info!("Resolved image digest: {resolved}");
                let resolved_msg = format!("Resolved: {resolved}");
                self.insert_log(deployment_id, Some(step_id), "info", &resolved_msg).await;
                self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", &resolved_msg).await;
                Ok(resolved)
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
                        self.publish_step_log(org_id, project_id, service_id, deployment_id, step_id, "info", line).await;
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
                        .bind(uuid::Uuid::now_v7())
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
        let rows = sqlx::query_as::<_, (String, bool)>(
            "SELECT hostname, tls_enabled
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

        // Query the service's ports so we can set an explicit Traefik backend port.
        // Without this label Traefik auto-detects the port after each restart, which
        // is unreliable when the container image has no EXPOSE or has multiple ports —
        // the root cause of the 521 after a Shipyard update.
        let ports_json: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT ports FROM services WHERE id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .flatten();

        let backend_port: Option<u16> = ports_json
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .and_then(|s| Self::parse_port_spec(s))
            .map(|ps| ps.target);

        // Stable named service that all routers for this Shipyard service share.
        // Using a prefix of the service UUID keeps it unique and Traefik-label-safe.
        let svc_name = format!("svc{}", &service_id.simple().to_string()[..8]);

        labels.insert("traefik.enable".to_string(), "true".to_string());

        if let Some(port) = backend_port {
            labels.insert(
                format!("traefik.http.services.{svc_name}.loadbalancer.server.port"),
                port.to_string(),
            );
            tracing::info!("Traefik backend port explicitly set to {port} (service {svc_name})");
        } else {
            tracing::warn!("No port configured for service {service_id} — Traefik will auto-detect (may be unreliable)");
        }

        for (hostname, tls_enabled) in &rows {
            // Derive a unique router name from the hostname so that two different
            // domains on the same service never share the same YAML/label key.
            let rn = hostname_to_router_name(hostname);
            let convenience = is_convenience_domain(hostname);

            if convenience {
                // Convenience domains (nip.io / traefik.me): HTTPS with self-signed cert.
                // The `web` entrypoint has a global HTTP→HTTPS redirect, so a service
                // router on `web` is redundant and gives Traefik extra ambiguous backend
                // auto-detection to do — skip it entirely for nip.io domains.
                let tls_rn = format!("{rn}-tls");
                labels.insert(
                    format!("traefik.http.routers.{tls_rn}.rule"),
                    format!("Host(`{hostname}`)"),
                );
                labels.insert(
                    format!("traefik.http.routers.{tls_rn}.entrypoints"),
                    "websecure".to_string(),
                );
                labels.insert(
                    format!("traefik.http.routers.{tls_rn}.tls"),
                    "true".to_string(),
                );
                labels.insert(
                    format!("traefik.http.routers.{tls_rn}.service"),
                    svc_name.clone(),
                );
            } else {
                // HTTP router (the global entrypoint redirect forwards HTTP → HTTPS
                // when TLS is active, so we don't need an explicit redirect middleware).
                labels.insert(
                    format!("traefik.http.routers.{rn}.rule"),
                    format!("Host(`{hostname}`)"),
                );
                labels.insert(
                    format!("traefik.http.routers.{rn}.entrypoints"),
                    "web".to_string(),
                );
                labels.insert(
                    format!("traefik.http.routers.{rn}.service"),
                    svc_name.clone(),
                );

                if *tls_enabled {
                    // Real domain with TLS: use Let's Encrypt.
                    let tls_rn = format!("{rn}-tls");
                    labels.insert(
                        format!("traefik.http.routers.{tls_rn}.rule"),
                        format!("Host(`{hostname}`)"),
                    );
                    labels.insert(
                        format!("traefik.http.routers.{tls_rn}.entrypoints"),
                        "websecure".to_string(),
                    );
                    labels.insert(
                        format!("traefik.http.routers.{tls_rn}.tls"),
                        "true".to_string(),
                    );
                    labels.insert(
                        format!("traefik.http.routers.{tls_rn}.tls.certresolver"),
                        "letsencrypt".to_string(),
                    );
                    labels.insert(
                        format!("traefik.http.routers.{tls_rn}.service"),
                        svc_name.clone(),
                    );
                }
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

        // Read the intended replica count, port config, and resource limits from the DB.
        let svc_row = sqlx::query_as::<_, (i32, Option<serde_json::Value>, Option<f64>, Option<i64>)>(
            "SELECT replicas, ports, cpu_limit, memory_limit_mb FROM services WHERE id = $1",
        )
        .bind(service_id)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

        let (db_replicas, db_ports_json, db_cpu_limit, db_mem_limit) = svc_row.unwrap_or((1, None, None, None));
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

        let resources = if db_cpu_limit.is_some() || db_mem_limit.is_some() {
            Some(ResourceSpec {
                cpu_limit: db_cpu_limit,
                memory_limit_mb: db_mem_limit.map(|m| m as u64),
                cpu_reservation: None,
                memory_reservation_mb: None,
            })
        } else {
            None
        };

        let spec = ServiceSpec {
            name: docker_svc_name.clone(),
            image: image_ref.to_string(),
            replicas: intended_replicas,
            env: env_vars,
            labels,
            mounts,
            networks,
            ports: swarm_ports,
            resources,
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
        let _ = self.docker.pull_image("alpine/socat", "latest", None).await;

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

/// Sanitize a hostname into a valid Traefik router/service name (ASCII alphanum + hyphens).
/// Must be unique per hostname so no two domains share the same YAML key.
fn hostname_to_router_name(hostname: &str) -> String {
    hostname
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_convenience_domain(hostname: &str) -> bool {
    hostname.ends_with(".nip.io") || hostname.ends_with(".traefik.me")
}

/// Map a `docker compose ps` State/Status string to a `container_status` enum value.
/// Remove ANSI escape sequences from a string (colour codes, cursor moves, etc.).
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC [ ... final-byte  (CSI sequences)
            if chars.peek() == Some(&'[') {
                chars.next();
                for ch in chars.by_ref() {
                    if ch.is_ascii_alphabetic() { break; }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

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

