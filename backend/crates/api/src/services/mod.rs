use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::{ApiResponse, MqttPayload};
use shipyard_db::models::{Service, ServiceEnv};
use shipyard_mqtt::topics;

use crate::auth::AuthUser;
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
            "/projects/:project_id/services/:service_id/webhook",
            get(get_webhook),
        )
        .route(
            "/projects/:project_id/services/:service_id/webhook/rotate",
            post(rotate_webhook),
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
        "SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at
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
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "app:project:service:write").await.map_err(ApiAppError)?;

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

    let service_id = Uuid::new_v4();
    let directory_path = format!(
        "{}/{}/{}/{}",
        state.config.data_dir, project_id, service_id, body.slug
    );

    let image = body.image.clone().unwrap_or_default();
    let ports = serde_json::to_value(&body.ports).unwrap_or_default();
    let replicas = body.replicas.max(1);
    let service = sqlx::query_as::<_, Service>(
        "INSERT INTO services (id, project_id, name, slug, type, image, git_repo_url, git_branch, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5::service_type, $6, $7, $8, $9, $10, 'stopped', $11, NULL, NULL, NULL, NOW(), NOW())
         RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at",
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
    let webhook_token = Uuid::new_v4().to_string().replace('-', "");
    upsert_webhook_token(&state.db, &state.config.auth.secret_key, service_id, &webhook_token).await.ok();

    crate::middleware::audit::write_audit_log(
        &state.db,
        Some(auth_user.user_id),
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
        "SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at
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
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "app:project:service:write").await.map_err(ApiAppError)?;

    if body.name.is_none() && body.status.is_none() && body.replicas.is_none() && body.ports.is_none() && body.image.is_none() {
        return Err(ApiAppError(AppError::BadRequest(
            "At least one field must be provided".to_string(),
        )));
    }

    // Fetch current service first
    let current = sqlx::query_as::<_, Service>(
        "SELECT id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at
         FROM services
         WHERE id = $1 AND project_id = $2",
    )
    .bind(service_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id))))?;

    let new_name = body.name.unwrap_or(current.name);
    let new_status = body.status.unwrap_or(current.status);
    let new_replicas = body.replicas.unwrap_or(current.replicas).max(0);
    let new_ports = match body.ports {
        Some(p) => serde_json::to_value(p).unwrap_or(current.ports),
        None => current.ports,
    };
    let new_image = body.image.unwrap_or(current.image);
    let new_cpu = match body.cpu_limit { Some(v) => Some(v), None => current.cpu_limit };
    let new_mem = match body.memory_limit_mb { Some(v) => Some(v), None => current.memory_limit_mb };

    let service = sqlx::query_as::<_, Service>(
        "UPDATE services
         SET name = $1, status = $2, replicas = $3, ports = $4, image = $5,
             cpu_limit = $6, memory_limit_mb = $7, updated_at = NOW()
         WHERE id = $8 AND project_id = $9
         RETURNING id, project_id, name, slug, type::text AS type, image, git_repo_url, git_branch, directory_path, ports, status, replicas, cpu_limit, memory_limit_mb, service_parent_id, created_at, updated_at",
    )
    .bind(&new_name)
    .bind(&new_status)
    .bind(new_replicas)
    .bind(&new_ports)
    .bind(&new_image)
    .bind(new_cpu)
    .bind(new_mem)
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
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "app:project:service:delete").await.map_err(ApiAppError)?;

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
    } else {
        // Non-compose: remove Swarm service
        let docker_svc_name = format!("{}-{}", state.config.docker.label_prefix, service_id);
        match state.docker.remove_service(&docker_svc_name).await {
            Ok(()) => tracing::info!(%service_id, "Docker swarm service removed"),
            Err(e) => tracing::warn!(%service_id, "Docker remove_service: {e}"),
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
        Some(auth_user.user_id),
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
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "app:project:service:write").await.map_err(ApiAppError)?;
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
        .bind(Uuid::new_v4())
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

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("service.env.changed")
            .with_meta(serde_json::json!({ "service_id": service_id }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
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
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "app:project:service:write").await.map_err(ApiAppError)?;

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
    .bind(Uuid::new_v4())
    .bind(service_id)
    .bind(&body.key)
    .bind(&encrypted)
    .bind(body.is_secret)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("service.env.changed")
            .with_meta(serde_json::json!({ "service_id": service_id }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(ServiceEnvResponse::from_env(env, secret_key)))))
}

/// DELETE /projects/:project_id/services/:service_id/env/:key
async fn delete_env(
    auth_user: AuthUser,
    Path((project_id, service_id, key)): Path<(Uuid, Uuid, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    rbac::require_project_permission(&state.db, auth_user.user_id, project_id, "app:project:service:write").await.map_err(ApiAppError)?;
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

    if let Ok(Some((org_id,))) = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("service.env.changed")
            .with_meta(serde_json::json!({ "service_id": service_id }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Env var deleted successfully"
    }))))
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
    .bind(Uuid::new_v4())
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
        &state.db, auth_user.user_id, project_id, "app:project:service:write",
    ).await.map_err(ApiAppError)?;
    verify_service_project(&state.db, service_id, project_id).await?;

    let token = Uuid::new_v4().to_string().replace('-', "");
    upsert_webhook_token(&state.db, &state.config.auth.secret_key, service_id, &token).await?;

    tracing::info!(service_id = %service_id, "webhook token rotated");

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "token": token,
        "service_id": service_id,
    }))))
}
