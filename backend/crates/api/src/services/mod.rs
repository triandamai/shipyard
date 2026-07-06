use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::{ApiResponse, MqttPayload};
use shipyard_db::models::{Service, ServiceEnv};
use shipyard_mqtt::topics;

use crate::auth::{decode_token, AuthUser};
use crate::error::ApiAppError;
use crate::middleware::rbac;
use crate::AppState;

// ─── RBAC helper ──────────────────────────────────────────────────────────────

/// Verify the authenticated user is a member of the org that owns `project_id`.
async fn check_project_access(
    db: &sqlx::PgPool,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<(), ApiAppError> {
    let org_id: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (org_id,) = org_id.ok_or_else(|| ApiAppError(AppError::NotFound(
        format!("Project '{}' not found", project_id)
    )))?;

    let is_member: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if is_member.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "You are not a member of this organization".to_string(),
        )));
    }
    Ok(())
}

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateServiceRequest {
    pub name: String,
    pub slug: String,
    #[serde(rename = "type")]
    pub service_type: String,
    pub image: Option<String>,
    pub git_repo_url: Option<String>,
    #[serde(default = "default_branch")]
    pub git_branch: String,
    /// Port bindings in "hostPort:containerPort" format, e.g. ["5432:5432"]
    #[serde(default)]
    pub ports: Vec<String>,
    /// Number of replicas (default 1)
    #[serde(default = "default_replicas")]
    pub replicas: i32,
}

fn default_branch() -> String { "main".to_string() }

fn default_replicas() -> i32 { 1 }

#[derive(Debug, Deserialize)]
pub struct UpdateServiceRequest {
    pub name: Option<String>,
    pub status: Option<String>,
    pub replicas: Option<i32>,
    pub ports: Option<Vec<String>>,
    pub image: Option<String>,
    pub cpu_limit: Option<f64>,
    pub memory_limit_mb: Option<i64>,
    pub git_branch: Option<String>,
    pub auto_deploy: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct EnvVarRequest {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub is_secret: bool,
}

/// ServiceEnv response that masks secrets.
#[derive(Debug, Serialize)]
pub struct ServiceEnvResponse {
    pub id: Uuid,
    pub service_id: Uuid,
    pub key: String,
    pub value: String,
    pub is_secret: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl ServiceEnvResponse {
    fn from_env(env: ServiceEnv, secret_key: &str) -> Self {
        let value = if env.is_secret {
            "***".to_string()
        } else {
            shipyard_common::crypto::decrypt_or_passthrough(secret_key, &env.value_encrypted)
        };
        Self {
            id: env.id,
            service_id: env.service_id,
            key: env.key,
            value,
            is_secret: env.is_secret,
            created_at: env.created_at,
        }
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/:project_id/services",
            get(list_services).post(create_service),
        )
        .route(
            "/projects/:project_id/services/:service_id",
            get(get_service).put(update_service).delete(delete_service),
        )
        .route(
            "/projects/:project_id/services/:service_id/env",
            get(list_env).put(bulk_update_env).post(add_env),
        )
        .route(
            "/projects/:project_id/services/:service_id/env/:key",
            delete(delete_env),
        )
        .route(
            "/projects/:project_id/services/:service_id/env/:env_id/reveal",
            get(reveal_env),
        )
        .route(
            "/projects/:project_id/services/:service_id/webhook",
            get(get_webhook),
        )
        .route(
            "/projects/:project_id/services/:service_id/webhook/rotate",
            post(rotate_webhook),
        )
        .route(
            "/projects/:project_id/services/:service_id/connection",
            get(get_connection_info),
        )
        .route(
            "/projects/:project_id/services/:service_id/replicas",
            get(list_replicas),
        )
        .route(
            "/projects/:project_id/services/:service_id/exec/token",
            post(exec_token),
        )
        .route(
            "/projects/:project_id/services/:service_id/exec",
            get(exec_ws),
        )
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /projects/:project_id/services
async fn list_services(
    auth_user: AuthUser,
    Path(project_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Service>>>, ApiAppError> {
    check_project_access(&state.db, auth_user.user_id, project_id).await?;

    let services = sqlx::query_as::<_, Service>(
        "SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at
         FROM services
         WHERE project_id = $1
         ORDER BY created_at ASC",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(services)))
}

/// POST /projects/:project_id/services
async fn create_service(
    auth_user: AuthUser,
    Path(project_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<CreateServiceRequest>,
) -> Result<(StatusCode, Json<ApiResponse<Service>>), ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "service:write").await.map_err(ApiAppError)?;

    if body.name.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("name is required".to_string())));
    }
    if body.slug.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("slug is required".to_string())));
    }
    if body.service_type.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("type is required".to_string())));
    }

    // Verify project exists
    let project_exists: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if project_exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Project '{}' not found",
            project_id
        ))));
    }

    let service_id = Uuid::now_v7();
    let directory_path = format!(
        "{}/{}/{}/{}",
        state.config.data_dir, project_id, service_id, body.slug
    );

    let image = body.image.clone().unwrap_or_default();
    let ports = serde_json::to_value(&body.ports).unwrap_or_default();
    let replicas = body.replicas.max(1);
    let service = sqlx::query_as::<_, Service>(
        "INSERT INTO services (id, project_id, name, slug, type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5::service_type, $6, $7, $8, true, $9, $10, 'stopped', $11, NULL, NULL, NULL, NOW(), NOW())
         RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at",
    )
    .bind(service_id)
    .bind(project_id)
    .bind(&body.name)
    .bind(&body.slug)
    .bind(&body.service_type)
    .bind(&image)
    .bind(&body.git_repo_url)
    .bind(&body.git_branch)
    .bind(&directory_path)
    .bind(&ports)
    .bind(replicas)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiAppError(AppError::Conflict(format!(
                "A service with slug '{}' already exists in this project",
                body.slug
            )))
        } else if msg.contains("invalid input value for enum") {
            ApiAppError(AppError::BadRequest(format!(
                "Invalid service type: '{}'",
                body.service_type
            )))
        } else {
            ApiAppError(AppError::Database(msg))
        }
    })?;

    // Auto-generate a webhook token so the URL is ready immediately.
    let webhook_token = Uuid::now_v7().to_string().replace('-', "");
    upsert_webhook_token(&state.db, &state.config.auth.secret_key, service_id, &webhook_token).await.ok();

    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth_user,
        "create_service",
        Some("service"),
        Some(service.id),
        None,
        Some(serde_json::json!({ "name": service.name, "project_id": project_id })),
    ).await;

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("service.created")
            .with_meta(serde_json::json!({
                "service_id": service.id,
                "service_name": service.name,
                "service_type": service.service_type,
            }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(service))))
}

/// GET /projects/:project_id/services/:service_id
async fn get_service(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Service>>, ApiAppError> {
    check_project_access(&state.db, auth_user.user_id, project_id).await?;
    let service = sqlx::query_as::<_, Service>(
        "SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at
         FROM services
         WHERE id = $1 AND project_id = $2",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id))))?;

    Ok(Json(ApiResponse::ok(service)))
}

/// PUT /projects/:project_id/services/:service_id
async fn update_service(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<UpdateServiceRequest>,
) -> Result<Json<ApiResponse<Service>>, ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "service:write").await.map_err(ApiAppError)?;

    // Fetch current service first
    let current = sqlx::query_as::<_, Service>(
        "SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at
         FROM services
         WHERE id = $1 AND project_id = $2",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id))))?;

    let new_name       = body.name.unwrap_or(current.name);
    let new_status     = body.status.unwrap_or(current.status);
    let new_replicas   = body.replicas.unwrap_or(current.replicas).max(0);
    let new_ports      = body.ports.map(|p| serde_json::to_value(p).unwrap_or(current.ports.clone())).unwrap_or(current.ports);
    let new_image      = body.image.unwrap_or(current.image);
    let new_cpu        = body.cpu_limit.or(current.cpu_limit);
    let new_mem        = body.memory_limit_mb.or(current.memory_limit_mb);
    let new_git_branch = body.git_branch.unwrap_or(current.git_branch);
    let new_auto_deploy = body.auto_deploy.unwrap_or(current.auto_deploy);

    let service = sqlx::query_as::<_, Service>(
        "UPDATE services
         SET name = $1, status = $2, replicas = $3, ports = $4, image = $5,
             cpu_limit = $6, memory_limit_mb = $7, git_branch = $8, auto_deploy = $9,
             updated_at = NOW()
         WHERE id = $10 AND project_id = $11
         RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, auto_deploy, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at",
    )
    .bind(&new_name)
    .bind(&new_status)
    .bind(new_replicas)
    .bind(&new_ports)
    .bind(&new_image)
    .bind(new_cpu)
    .bind(new_mem)
    .bind(&new_git_branch)
    .bind(new_auto_deploy)
    .bind(service_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id))))?;

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("service.updated")
            .with_meta(serde_json::json!({
                "service_id": service.id,
                "service_name": service.name,
            }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok(Json(ApiResponse::ok(service)))
}

/// DELETE /projects/:project_id/services/:service_id
async fn delete_service(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "service:delete").await.map_err(ApiAppError)?;

    // Fetch service info before deleting.
    let svc_info: Option<(String, String, String)> = sqlx::query_as(
        "SELECT type::text, slug, COALESCE(directory_path, '') FROM services WHERE id = $1 AND project_id = $2",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (svc_type, service_slug, directory_path) = svc_info
        .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id))))?;

    if svc_type == "docker_compose" {
        // --- Run `docker compose down` to stop containers and remove compose networks ---
        // Find compose file (best effort — if directory doesn't exist, skip).
        let compose_candidates = [
            format!("{directory_path}/docker-compose.yml"),
            format!("{directory_path}/docker-compose.yaml"),
            format!("{directory_path}/compose.yml"),
            format!("{directory_path}/compose.yaml"),
            // also check .shipyard-compose.yml written by the preprocessor
            format!("{directory_path}/.shipyard-compose.yml"),
        ];
        let compose_file = compose_candidates.iter().find(|p| std::path::Path::new(p).exists());
        if let Some(cf) = compose_file {
            match tokio::process::Command::new("docker")
                .args(["compose", "-f", cf, "down", "--remove-orphans"])
                .current_dir(&directory_path)
                .output()
                .await
            {
                Ok(out) if out.status.success() => {
                    tracing::info!(%service_id, "docker compose down succeeded");
                }
                Ok(out) => {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    tracing::warn!(%service_id, "docker compose down: {stderr}");
                }
                Err(e) => tracing::warn!(%service_id, "docker compose down spawn failed: {e}"),
            }
        }

        // --- Collect child service IDs for Swarm + Traefik cleanup ---
        let children: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, slug FROM services WHERE service_parent_id = $1",
        )
        .bind(service_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        for (child_id, child_slug) in &children {
            // Swarm remove (best-effort; compose services are usually standalone)
            let name = format!("{}-{}", state.config.docker.label_prefix, child_id);
            if let Err(e) = state.docker.remove_service(&name).await {
                tracing::debug!(%child_id, "child swarm remove: {e}");
            }
            // Traefik dynamic config (best-effort)
            if let Some(dir) = state.config.traefik.dynamic_config_dir.as_deref() {
                let path = std::path::Path::new(dir).join(format!("{child_slug}.yml"));
                let _ = tokio::fs::remove_file(&path).await;
            }
        }

        // --- Delete all child services (CASCADE handles their containers/envs/etc.) ---
        sqlx::query("DELETE FROM services WHERE service_parent_id = $1")
            .bind(service_id)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        // --- Delete networks that no longer belong to any service in this project ---
        sqlx::query(
            "DELETE FROM networks
             WHERE project_id = $1
               AND id NOT IN (SELECT network_id FROM service_networks sn
                              JOIN networks n2 ON n2.id = sn.network_id
                              WHERE n2.project_id = $1)",
        )
        .bind(project_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        tracing::info!(%service_id, "compose stack children + orphan networks deleted");

        // Remove compose directory (uploaded compose files, cloned git repo, etc.)
        if !directory_path.is_empty() {
            match tokio::fs::remove_dir_all(&directory_path).await {
                Ok(()) => tracing::info!(%service_id, "Removed compose dir: {directory_path}"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(%service_id, "Could not remove compose dir: {e}"),
            }
        }
    } else if svc_type == "static" {
        // Static site: remove served files + nginx conf, then reload nginx.
        // Domains and service_networks are handled by ON DELETE CASCADE in the DB.
        let sites_base = state.config.static_server.sites_dir
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}/static", state.config.data_dir));

        // Remove versioned site directory (all deploy versions + current symlink)
        let site_dir = format!("{sites_base}/{service_id}");
        match tokio::fs::remove_dir_all(&site_dir).await {
            Ok(()) => tracing::info!(%service_id, "Removed static site dir: {site_dir}"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(%service_id, "Could not remove static site dir: {e}"),
        }

        // Remove nginx site conf
        let conf_path = format!("{sites_base}/conf.d/{service_slug}.conf");
        match tokio::fs::remove_file(&conf_path).await {
            Ok(()) => tracing::info!(%service_id, "Removed nginx conf: {conf_path}"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(%service_id, "Could not remove nginx conf: {e}"),
        }

        // Remove uploaded zip artifacts
        let uploads_dir = format!("{sites_base}/uploads/{service_id}");
        match tokio::fs::remove_dir_all(&uploads_dir).await {
            Ok(()) => tracing::info!(%service_id, "Removed static uploads dir: {uploads_dir}"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(%service_id, "Could not remove static uploads dir: {e}"),
        }

        // Remove service data directory (deploy metadata, etc.)
        if !directory_path.is_empty() {
            match tokio::fs::remove_dir_all(&directory_path).await {
                Ok(()) => tracing::info!(%service_id, "Removed static service dir: {directory_path}"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(%service_id, "Could not remove static service dir: {e}"),
            }
        }

        // Reload nginx inside the container so the server block is gone (best-effort)
        let _ = tokio::process::Command::new("docker")
            .args(["exec", "shipyard-nginx-static", "nginx", "-s", "reload"])
            .output()
            .await;
    } else {
        // Non-compose, non-static: remove Docker Swarm service
        let docker_svc_name = format!("{}-{}", state.config.docker.label_prefix, service_id);
        match state.docker.remove_service(&docker_svc_name).await {
            Ok(()) => tracing::info!(%service_id, "Docker swarm service removed"),
            Err(e) => tracing::warn!(%service_id, "Docker remove_service: {e}"),
        }

        // Remove service data directory (source code, build artefacts, etc.)
        if !directory_path.is_empty() {
            match tokio::fs::remove_dir_all(&directory_path).await {
                Ok(()) => tracing::info!(%service_id, "Removed service dir: {directory_path}"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!(%service_id, "Could not remove service dir: {e}"),
            }
        }
    }

    let rows_affected = sqlx::query(
        "DELETE FROM services WHERE id = $1 AND project_id = $2",
    )
    .bind(service_id)
    .bind(project_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Service '{}' not found",
            service_id
        ))));
    }

    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth_user,
        "delete_service",
        Some("service"),
        Some(service_id),
        None,
        Some(serde_json::json!({ "project_id": project_id })),
    ).await;

    // Remove Traefik dynamic config for this (root) service (best-effort).
    if let Some(dir) = state.config.traefik.dynamic_config_dir.as_deref() {
        let path = std::path::Path::new(dir).join(format!("{service_slug}.yml"));
        match tokio::fs::remove_file(&path).await {
            Ok(()) => tracing::info!(%service_id, "Removed Traefik dynamic config: {path:?}"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(%service_id, "Could not remove Traefik dynamic config: {e}"),
        }
    }

    // Publish MQTT topology event so connected clients refresh their topology view.
    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = shipyard_mqtt::topics::topology(org_id, project_id);
        let payload = MqttPayload::new("service.deleted")
            .with_meta(serde_json::json!({ "service_id": service_id }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Service deleted successfully"
    }))))
}

// ─── Env Var Handlers ─────────────────────────────────────────────────────────

/// GET /projects/:project_id/services/:service_id/env
async fn list_env(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<ServiceEnvResponse>>>, ApiAppError> {
    check_project_access(&state.db, auth_user.user_id, project_id).await?;
    verify_service_project(&state.db, service_id, project_id).await?;

    let envs = sqlx::query_as::<_, ServiceEnv>(
        "SELECT id, service_id, key, value_encrypted, is_secret, created_at
         FROM service_envs
         WHERE service_id = $1
         ORDER BY key ASC",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let key = &state.config.auth.secret_key;
    let response: Vec<ServiceEnvResponse> = envs.into_iter().map(|e| ServiceEnvResponse::from_env(e, key)).collect();
    Ok(Json(ApiResponse::ok(response)))
}

/// PUT /projects/:project_id/services/:service_id/env — bulk replace all env vars
async fn bulk_update_env(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<Vec<EnvVarRequest>>,
) -> Result<Json<ApiResponse<Vec<ServiceEnvResponse>>>, ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "service:write").await.map_err(ApiAppError)?;
    verify_service_project(&state.db, service_id, project_id).await?;

    let secret_key = state.config.auth.secret_key.clone();

    // Validate all keys up-front before touching the DB.
    for item in &body {
        if item.key.is_empty() {
            return Err(ApiAppError(AppError::BadRequest("env var key cannot be empty".to_string())));
        }
    }

    // Delete + re-insert inside a single transaction so the service never ends
    // up with zero env vars if an insert fails mid-way.
    let mut tx = state.db.begin().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    sqlx::query("DELETE FROM service_envs WHERE service_id = $1")
        .bind(service_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let mut results: Vec<ServiceEnvResponse> = Vec::new();

    for item in &body {
        let encrypted = shipyard_common::crypto::encrypt_or_passthrough(&secret_key, &item.value);

        let env = sqlx::query_as::<_, ServiceEnv>(
            "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
             VALUES ($1, $2, $3, $4, $5, NOW())
             RETURNING id, service_id, key, value_encrypted, is_secret, created_at",
        )
        .bind(Uuid::now_v7())
        .bind(service_id)
        .bind(&item.key)
        .bind(&encrypted)
        .bind(item.is_secret)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        results.push(ServiceEnvResponse::from_env(env, &secret_key));
    }

    tx.commit().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    detect_and_store_platform_refs(&state.db, service_id, project_id, &state.config.auth.secret_key).await;

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        state.mqtt.publish_status(&topic, &MqttPayload::new("service.env.changed")
            .with_meta(serde_json::json!({ "service_id": service_id }))).await.ok();
        state.mqtt.publish_status(&topic, &MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({ "reason": "env_ref" }))).await.ok();
    }

    Ok(Json(ApiResponse::ok(results)))
}

/// POST /projects/:project_id/services/:service_id/env — add single env var
async fn add_env(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<EnvVarRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ServiceEnvResponse>>), ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "service:write").await.map_err(ApiAppError)?;

    if body.key.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("key is required".to_string())));
    }

    verify_service_project(&state.db, service_id, project_id).await?;

    let secret_key = &state.config.auth.secret_key;
    let encrypted = shipyard_common::crypto::encrypt_or_passthrough(secret_key, &body.value);

    let env = sqlx::query_as::<_, ServiceEnv>(
        "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW())
         ON CONFLICT (service_id, key) DO UPDATE
           SET value_encrypted = EXCLUDED.value_encrypted,
               is_secret = EXCLUDED.is_secret
         RETURNING id, service_id, key, value_encrypted, is_secret, created_at",
    )
    .bind(Uuid::now_v7())
    .bind(service_id)
    .bind(&body.key)
    .bind(&encrypted)
    .bind(body.is_secret)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    detect_and_store_platform_refs(&state.db, service_id, project_id, &state.config.auth.secret_key).await;

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        state.mqtt.publish_status(&topic, &MqttPayload::new("service.env.changed")
            .with_meta(serde_json::json!({ "service_id": service_id }))).await.ok();
        state.mqtt.publish_status(&topic, &MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({ "reason": "env_ref" }))).await.ok();
    }

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(ServiceEnvResponse::from_env(env, secret_key)))))
}

/// DELETE /projects/:project_id/services/:service_id/env/:key
async fn delete_env(
    auth_user: AuthUser,
    Path((project_id, service_id, key)): Path<(Uuid, Uuid, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "service:write").await.map_err(ApiAppError)?;
    verify_service_project(&state.db, service_id, project_id).await?;

    let rows_affected = sqlx::query(
        "DELETE FROM service_envs WHERE service_id = $1 AND key = $2",
    )
    .bind(service_id)
    .bind(&key)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Env var '{}' not found for service '{}'",
            key, service_id
        ))));
    }

    detect_and_store_platform_refs(&state.db, service_id, project_id, &state.config.auth.secret_key).await;

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        state.mqtt.publish_status(&topic, &MqttPayload::new("service.env.changed")
            .with_meta(serde_json::json!({ "service_id": service_id }))).await.ok();
        state.mqtt.publish_status(&topic, &MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({ "reason": "env_ref" }))).await.ok();
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Env var deleted successfully"
    }))))
}

/// GET /projects/:project_id/services/:service_id/env/:env_id/reveal
/// Returns the decrypted value for a single secret env var.
async fn reveal_env(
    auth_user: AuthUser,
    Path((project_id, service_id, env_id)): Path<(Uuid, Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "service:write").await.map_err(ApiAppError)?;
    verify_service_project(&state.db, service_id, project_id).await?;

    let row: Option<(String, bool)> = sqlx::query_as::<_, (String, bool)>(
        "SELECT value_encrypted, is_secret FROM service_envs WHERE id = $1 AND service_id = $2",
    )
    .bind(env_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (encrypted, is_secret) = row.ok_or_else(|| ApiAppError(AppError::NotFound(format!("Env var {env_id} not found"))))?;
    let value = shipyard_common::crypto::decrypt_or_passthrough(&state.config.auth.secret_key, &encrypted);
    let _ = is_secret; // always decrypt regardless

    Ok(Json(ApiResponse::ok(serde_json::json!({ "value": value }))))
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

async fn verify_service_project(
    db: &sqlx::PgPool,
    service_id: Uuid,
    project_id: Uuid,
) -> Result<(), ApiAppError> {
    let exists: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM services WHERE id = $1 AND project_id = $2",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Service '{}' not found in project '{}'",
            service_id, project_id
        ))));
    }
    Ok(())
}

// ─── Platform-ref detection ───────────────────────────────────────────────────

/// Extract the `{slug}` part from every `platform-{slug}` token in `value`.
/// Requires that the `p` of `platform-` is either at start-of-string or
/// preceded by a non-alphanumeric, non-hyphen character so we don't match
/// inside longer tokens like `no-platform-x`.
fn extract_platform_slugs(value: &str) -> Vec<String> {
    let prefix = "platform-";
    let mut slugs: Vec<String> = Vec::new();
    let mut search_from = 0;

    while search_from < value.len() {
        let Some(rel) = value[search_from..].find(prefix) else { break };
        let abs = search_from + rel;
        let prev_ok = abs == 0 || {
            let b = value.as_bytes()[abs - 1];
            !b.is_ascii_alphanumeric() && b != b'-'
        };
        let slug_start = abs + prefix.len();
        if prev_ok && slug_start < value.len() {
            let rest = &value[slug_start..];
            let end = rest
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '-')
                .unwrap_or(rest.len());
            if end > 0 && rest.as_bytes().first().map(|b| b.is_ascii_alphanumeric()).unwrap_or(false) {
                slugs.push(rest[..end].to_lowercase());
            }
        }
        search_from = abs + prefix.len();
    }

    slugs.sort();
    slugs.dedup();
    slugs
}

/// Re-scan ALL envs for this service, resolve `platform-{slug}` references
/// to services / networks / volumes in the same org, and rewrite `service_env_refs`.
pub(crate) async fn detect_and_store_platform_refs(db: &sqlx::PgPool, service_id: Uuid, project_id: Uuid, secret_key: &str) {
    if let Err(e) = do_detect_platform_refs(db, service_id, project_id, secret_key).await {
        tracing::error!(%service_id, "platform ref detection failed: {e}");
    }
}

async fn do_detect_platform_refs(db: &sqlx::PgPool, service_id: Uuid, project_id: Uuid, secret_key: &str) -> Result<(), sqlx::Error> {
    let (org_id,): (Uuid,) = sqlx::query_as(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_one(db)
    .await?;

    tracing::debug!(%service_id, %org_id, "scanning envs for platform refs");

    // ALL env values for this service (including secrets — pattern refs are logical, not data)
    let envs: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value_encrypted FROM service_envs WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_all(db)
    .await?;

    // Collect unique (env_key, slug) pairs — decrypt each value first
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (key, value_encrypted) in &envs {
        let value = shipyard_common::crypto::decrypt_or_passthrough(secret_key, value_encrypted);
        for slug in extract_platform_slugs(&value) {
            if !pairs.iter().any(|(_, s)| s == &slug) {
                tracing::info!(%service_id, key, %slug, "found platform ref");
                pairs.push((key.clone(), slug));
            }
        }
    }

        if pairs.is_empty() {
            tracing::debug!(%service_id, "no platform refs found in envs");
        }

        // Resolve each slug → resource (service > network > volume)
        let mut resolved: Vec<(String, String, String, Uuid, Uuid)> = Vec::new();

        for (env_key, slug) in &pairs {
            tracing::info!(%service_id, %slug, "resolving platform ref");
            let ref_val = format!("platform-{slug}");

            if let Some((rid, rpid)) = sqlx::query_as::<_, (Uuid, Uuid)>(
                "SELECT s.id, s.project_id FROM services s
                 JOIN projects p ON p.id = s.project_id
                 WHERE p.org_id = $1
                   AND (LOWER(s.slug) = $2 OR s.id::text = $2)
                 LIMIT 1",
            )
            .bind(org_id).bind(slug.as_str())
            .fetch_optional(db).await? {
                tracing::info!(%service_id, %slug, %rid, "resolved as service");
                resolved.push((env_key.clone(), ref_val, "service".into(), rid, rpid));
                continue;
            }
            tracing::debug!(%service_id, %slug, "not a service, trying network");

            if let Some((rid, rpid)) = sqlx::query_as::<_, (Uuid, Uuid)>(
                "SELECT n.id, n.project_id FROM networks n
                 JOIN projects p ON p.id = n.project_id
                 WHERE p.org_id = $1
                   AND (LOWER(REPLACE(n.name, ' ', '-')) = $2 OR n.id::text = $2)
                 LIMIT 1",
            )
            .bind(org_id).bind(slug.as_str())
            .fetch_optional(db).await? {
                tracing::info!(%service_id, %slug, %rid, "resolved as network");
                resolved.push((env_key.clone(), ref_val, "network".into(), rid, rpid));
                continue;
            }
            tracing::debug!(%service_id, %slug, "not a network, trying volume");

            if let Some((rid, rpid)) = sqlx::query_as::<_, (Uuid, Uuid)>(
                "SELECT v.id, COALESCE(v.project_id, s.project_id) AS project_id
                 FROM volumes v
                 LEFT JOIN services s ON s.id = v.service_id
                 JOIN projects p ON p.id = COALESCE(v.project_id, s.project_id)
                 WHERE p.org_id = $1
                   AND (LOWER(REPLACE(v.name, ' ', '-')) = $2 OR v.id::text = $2)
                 LIMIT 1",
            )
            .bind(org_id).bind(slug.as_str())
            .fetch_optional(db).await? {
                tracing::info!(%service_id, %slug, %rid, "resolved as volume");
                resolved.push((env_key.clone(), ref_val, "volume".into(), rid, rpid));
            } else {
                tracing::warn!(%service_id, %slug, %org_id, "platform ref slug not found in org");
            }
        }

        // Rewrite refs for this service
        sqlx::query("DELETE FROM service_env_refs WHERE service_id = $1")
            .bind(service_id)
            .execute(db)
            .await?;

        tracing::info!(%service_id, count = resolved.len(), "storing platform refs");

        for (env_key, ref_value, rtype, rid, rpid) in resolved {
            tracing::info!(%service_id, %ref_value, %rtype, %rid, "inserting env ref");
            sqlx::query(
                "INSERT INTO service_env_refs
                 (id, service_id, env_key, ref_value, resource_type, resource_id, resource_project_id, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
                 ON CONFLICT (service_id, ref_value) DO UPDATE
                     SET env_key = EXCLUDED.env_key,
                         resource_type = EXCLUDED.resource_type,
                         resource_id = EXCLUDED.resource_id,
                         resource_project_id = EXCLUDED.resource_project_id",
            )
            .bind(Uuid::now_v7())
            .bind(service_id)
            .bind(&env_key)
            .bind(&ref_value)
            .bind(&rtype)
            .bind(rid)
            .bind(rpid)
            .execute(db)
            .await?;
        }

        Ok(())
}

// ─── Webhook token helpers ────────────────────────────────────────────────────

async fn upsert_webhook_token(
    db: &sqlx::PgPool,
    secret_key: &str,
    service_id: Uuid,
    token: &str,
) -> Result<(), ApiAppError> {
    let encrypted = shipyard_common::crypto::encrypt_or_passthrough(secret_key, token);
    sqlx::query(
        "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
         VALUES ($1, $2, '__WEBHOOK_TOKEN__', $3, TRUE, NOW())
         ON CONFLICT (service_id, key) DO UPDATE
           SET value_encrypted = EXCLUDED.value_encrypted",
    )
    .bind(Uuid::now_v7())
    .bind(service_id)
    .bind(&encrypted)
    .execute(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    Ok(())
}

async fn read_webhook_token(
    db: &sqlx::PgPool,
    secret_key: &str,
    service_id: Uuid,
) -> Result<String, ApiAppError> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT value_encrypted FROM service_envs
         WHERE service_id = $1 AND key = '__WEBHOOK_TOKEN__'
         LIMIT 1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let stored = row.map(|(v,)| v).ok_or_else(|| {
        ApiAppError(AppError::NotFound(
            "No webhook token found for this service".to_string(),
        ))
    })?;
    Ok(shipyard_common::crypto::decrypt_or_passthrough(secret_key, &stored))
}

// ─── Webhook handlers ─────────────────────────────────────────────────────────

/// GET /projects/:project_id/services/:service_id/webhook
///
/// Returns the current (decrypted) webhook token so the frontend can build the URL.
async fn get_webhook(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    check_project_access(&state.db, auth_user.user_id, project_id).await?;
    verify_service_project(&state.db, service_id, project_id).await?;

    let token = read_webhook_token(&state.db, &state.config.auth.secret_key, service_id).await?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "token": token,
        "service_id": service_id,
    }))))
}

/// POST /projects/:project_id/services/:service_id/webhook/rotate
///
/// Generates a fresh token, replaces the old one, and returns the new value.
async fn rotate_webhook(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    rbac::require_project_permission(
        &state.db, auth_user.user_id, project_id, "service:write",
    ).await.map_err(ApiAppError)?;
    verify_service_project(&state.db, service_id, project_id).await?;

    let token = Uuid::now_v7().to_string().replace('-', "");
    upsert_webhook_token(&state.db, &state.config.auth.secret_key, service_id, &token).await?;

    tracing::info!(service_id = %service_id, "webhook token rotated");

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "token": token,
        "service_id": service_id,
    }))))
}

// ─── Connection info ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ConnectionInfo {
    /// DNS hostname reachable by other services on the same overlay network.
    host: String,
    port: u16,
    /// Best-guess connection URL template — replace USER/PASSWORD/DATABASE with env var values.
    url_template: String,
    /// Human-readable driver name (e.g. "PostgreSQL").
    driver: String,
}

fn infer_connection(image: &str, host: &str) -> ConnectionInfo {
    let base = image.split(':').next().unwrap_or(image).to_lowercase();
    let img = base.split('/').last().unwrap_or(&base);

    if img.contains("postgres") {
        return ConnectionInfo { host: host.into(), port: 5432, driver: "PostgreSQL".into(),
            url_template: format!("postgresql://USER:PASSWORD@{host}:5432/DATABASE") };
    }
    if img.contains("mysql") {
        return ConnectionInfo { host: host.into(), port: 3306, driver: "MySQL".into(),
            url_template: format!("mysql://USER:PASSWORD@{host}:3306/DATABASE") };
    }
    if img.contains("mariadb") {
        return ConnectionInfo { host: host.into(), port: 3306, driver: "MariaDB".into(),
            url_template: format!("mysql://USER:PASSWORD@{host}:3306/DATABASE") };
    }
    if img.contains("redis") {
        return ConnectionInfo { host: host.into(), port: 6379, driver: "Redis".into(),
            url_template: format!("redis://{host}:6379") };
    }
    if img.contains("mongo") {
        return ConnectionInfo { host: host.into(), port: 27017, driver: "MongoDB".into(),
            url_template: format!("mongodb://USER:PASSWORD@{host}:27017/DATABASE") };
    }
    if img.contains("rabbitmq") {
        return ConnectionInfo { host: host.into(), port: 5672, driver: "RabbitMQ".into(),
            url_template: format!("amqp://USER:PASSWORD@{host}:5672") };
    }
    if img.contains("minio") {
        return ConnectionInfo { host: host.into(), port: 9000, driver: "MinIO".into(),
            url_template: format!("http://{host}:9000") };
    }
    ConnectionInfo { host: host.into(), port: 80, driver: "TCP".into(),
        url_template: format!("http://{host}:PORT") }
}

async fn get_connection_info(
    auth_user: AuthUser,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ConnectionInfo>>, ApiAppError> {
    check_project_access(&state.db, auth_user.user_id, project_id).await?;

    let (image, ports): (String, serde_json::Value) = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT image, ports FROM services WHERE id = $1 AND project_id = $2",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service {service_id} not found"))))?;

    let host = format!("{}-{}", state.config.docker.label_prefix, service_id);
    let mut info = infer_connection(&image, &host);

    // Override port if the service has an explicit port configured
    if let Some(first) = ports.as_array().and_then(|a| a.first()) {
        let raw = first.as_str().unwrap_or("").split(':').last().unwrap_or("").split('/').next().unwrap_or("");
        if let Ok(p) = raw.parse::<u16>() {
            if p > 0 {
                let default_port = infer_connection(&image, &host).port;
                info.url_template = info.url_template.replace(
                    &format!(":{default_port}"), &format!(":{p}"),
                );
                info.port = p;
            }
        }
    }

    // Substitute real env var values into the URL template so the user gets a
    // ready-to-use connection string instead of literal USER/PASSWORD/DATABASE
    // placeholders. The endpoint is authenticated so exposing credentials is fine.
    let envs: Vec<ServiceEnv> = sqlx::query_as::<_, ServiceEnv>(
        "SELECT id, service_id, key, value_encrypted, is_secret, created_at
         FROM service_envs WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let secret_key = &state.config.auth.secret_key;
    // Build key → plaintext value map (decrypt everything for the connection string)
    let env_map: std::collections::HashMap<String, String> = envs
        .into_iter()
        .map(|e| {
            let v = shipyard_common::crypto::decrypt_or_passthrough(secret_key, &e.value_encrypted);
            (e.key, v)
        })
        .collect();

    // Each entry: (template placeholder, list of env var keys to try in order)
    let substitutions: &[(&str, &[&str])] = &[
        ("USER",     &["POSTGRES_USER", "MYSQL_USER", "MONGO_INITDB_ROOT_USERNAME"]),
        ("PASSWORD", &["POSTGRES_PASSWORD", "MYSQL_PASSWORD", "MONGO_INITDB_ROOT_PASSWORD", "REDIS_PASSWORD"]),
        ("DATABASE", &["POSTGRES_DB", "MYSQL_DATABASE", "MONGO_INITDB_DATABASE"]),
    ];
    for (placeholder, keys) in substitutions {
        if let Some(val) = keys.iter().find_map(|k| env_map.get(*k)) {
            if !val.is_empty() {
                info.url_template = info.url_template.replace(placeholder, val);
            }
        }
    }

    Ok(Json(ApiResponse::ok(info)))
}

// ─── Exec token ───────────────────────────────────────────────────────────────

/// POST /projects/:project_id/services/:service_id/exec/token
///
/// Issues a short-lived JWT (5 min) for use as the `?token=` query param on
/// the WebSocket exec endpoint. Calling this through the normal /api proxy lets
/// the browser use its httponly session cookie, so the JS client doesn't need
/// to carry its own copy of the long-lived access token.
async fn exec_token(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((project_id, _service_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    check_project_access(&state.db, auth.user_id, project_id).await?;

    let token = crate::auth::create_access_token(
        auth.user_id,
        &auth.email,
        &state.config.auth.jwt_secret,
        300, // 5 minutes
    )
    .map_err(|e| ApiAppError(e))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "token": token }))))
}

// ─── Replicas ─────────────────────────────────────────────────────────────────

/// GET /projects/:project_id/services/:service_id/replicas
///
/// Returns the list of live Swarm tasks (one per replica) for the service,
/// including the container ID needed to open an exec session.
async fn list_replicas(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    rbac::require_project_access(&state.db, auth.user_id, project_id).await
        .map_err(ApiAppError)?;

    let local_node_id = state.docker.swarm_info().await
        .map(|i| i.node_id)
        .unwrap_or_default();

    let swarm_name = format!("{}-{}", state.config.docker.label_prefix, service_id);
    let tasks = state.docker.list_tasks(&swarm_name).await.unwrap_or_default();

    let items: Vec<serde_json::Value> = tasks.into_iter().map(|t| {
        let is_local = !local_node_id.is_empty()
            && t.node_id.as_deref().map(|n| n == local_node_id).unwrap_or(true);
        serde_json::json!({
            "id":             t.id,
            "slot":           t.slot,
            "node_id":        t.node_id,
            "container_id":   t.container_id,
            "status":         t.status,
            "image":          t.image,
            "is_local_node":  is_local,
        })
    }).collect();

    Ok(Json(ApiResponse::ok(items)))
}

// ─── Exec WebSocket ───────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ExecParams {
    /// Short-lived JWT passed as a query param (WebSocket API has no custom headers).
    token:        String,
    container_id: String,
    #[serde(default = "default_cmd")]
    cmd:          String,
    #[serde(default = "default_cols")]
    cols:         u16,
    #[serde(default = "default_rows")]
    rows:         u16,
}
fn default_cmd()  -> String { "/bin/sh".to_string() }
fn default_cols() -> u16    { 80 }
fn default_rows() -> u16    { 24 }

/// GET /projects/:project_id/services/:service_id/exec  (WebSocket upgrade)
///
/// Bridges the WebSocket to a `docker exec` PTY in the specified container.
///
/// Protocol:
///   Client → Server  binary frame = raw keystrokes (stdin)
///   Client → Server  text frame   = JSON `{"type":"resize","cols":N,"rows":N}`
///   Server → Client  binary frame = raw stdout/stderr bytes
///   Server → Client  text frame   = JSON `{"type":"error","message":"..."}`
async fn exec_ws(
    State(state): State<AppState>,
    Path((project_id, service_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<ExecParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Validate token before upgrading — reject invalid creds before WS handshake.
    let claims = match decode_token(&params.token, &state.config.auth.jwt_secret) {
        Ok(c) => c,
        Err(_) => {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    };

    let user_id = Uuid::parse_str(&claims.sub).unwrap_or_default();
    ws.on_upgrade(move |socket| {
        handle_exec_socket(socket, state, project_id, service_id, params, user_id)
    })
}

async fn handle_exec_socket(
    socket:     WebSocket,
    state:      AppState,
    _project_id: Uuid,
    service_id: Uuid,
    params:     ExecParams,
    user_id:    Uuid,
) {
    let (mut ws_sink, mut ws_stream) = socket.split();

    // Verify container is on the local node before attempting exec.
    // exec_container uses the local Docker socket and fails for containers on worker nodes.
    let local_node_id = state.docker.swarm_info().await
        .map(|i| i.node_id)
        .unwrap_or_default();

    if !local_node_id.is_empty() {
        let row: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT node_id FROM containers WHERE docker_container_id = $1",
        )
        .bind(&params.container_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);

        if let Some((Some(ref node_id),)) = row {
            if !node_id.is_empty() && node_id != &local_node_id {
                let _ = ws_sink.send(Message::Text(
                    serde_json::json!({
                        "type": "error",
                        "message": format!(
                            "This container is running on a different Swarm node ({}). \
                             Terminal is only available for containers on the manager node.",
                            &node_id[..node_id.len().min(12)]
                        )
                    })
                    .to_string(),
                ))
                .await;
                return;
            }
        }
    }

    let cmd: Vec<String> = params.cmd.split_whitespace().map(String::from).collect();

    let exec_handle = match state.docker.exec_container(
        &params.container_id, cmd, params.cols, params.rows,
    ).await {
        Ok(h) => h,
        Err(e) => {
            let _ = ws_sink.send(Message::Text(
                serde_json::json!({"type":"error","message": e.to_string()}).to_string()
            )).await;
            return;
        }
    };

    let exec_id    = exec_handle.exec_id;
    let mut output = exec_handle.output;
    let mut stdin  = exec_handle.stdin;

    // Write audit log (fire-and-forget) — WS path has no AuthUser, use bare variant
    crate::middleware::audit::write_audit_log_user(
        &state.db, user_id, "", "exec_container", Some("service"), Some(service_id),
        None, None,
    ).await;

    // Task 1: container stdout/stderr → WebSocket client
    let out_task = tokio::spawn(async move {
        while let Some(chunk) = output.next().await {
            if chunk.is_empty() { continue; }
            if ws_sink.send(Message::Binary(chunk.to_vec())).await.is_err() {
                break;
            }
        }
        let _ = ws_sink.close().await;
    });

    // Task 2: WebSocket client → container stdin; resize messages handled inline
    let docker = state.docker.clone();
    let exec_id_clone = exec_id.clone();
    let in_task = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt as _;
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Binary(data) => {
                    if stdin.write_all(&data).await.is_err() { break; }
                }
                Message::Text(text) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if v["type"] == "resize" {
                            let cols = v["cols"].as_u64().unwrap_or(80) as u16;
                            let rows = v["rows"].as_u64().unwrap_or(24) as u16;
                            let _ = docker.resize_exec(&exec_id_clone, cols, rows).await;
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        // Dropping stdin signals EOF to the container process.
        drop(stdin);
    });

    // Drive both tasks; cancel the other when one finishes.
    tokio::select! {
        _ = out_task => {}
        _ = in_task  => {}
    }
}
