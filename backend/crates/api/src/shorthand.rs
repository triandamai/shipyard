//! Shorthand routes — service-scoped paths without /projects/:pid/ prefix.
//!
//! All service UUIDs are globally unique so /services/:id/... works.
//! Deployment-only routes (/deployments/:id/...) look up the service from the
//! deployment row to verify access.
//! The long-form project-scoped routes remain for backwards compatibility.

use std::convert::Infallible;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::{ApiResponse, MqttPayload};
use shipyard_db::models::{Deployment, DeploymentLog, DeploymentStep, ServiceEnv};
use shipyard_docker;
use shipyard_engine::DeploymentEngine;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac::{require_service_access, require_service_permission};
use crate::AppState;

// ─── Local request/response types ────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct DeployBody {
    #[serde(default = "default_ref")]
    source_ref: String,
}
fn default_ref() -> String { "manual".to_string() }

#[derive(Debug, Deserialize)]
struct LogsQuery {
    pub level: Option<String>,
    pub step_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct EnvVarRequest {
    key: String,
    value: String,
    #[serde(default)]
    is_secret: bool,
}

#[derive(Debug, Serialize)]
struct EnvVarResponse {
    id: Uuid,
    service_id: Uuid,
    key: String,
    /// Field name matches the frontend `ServiceEnv.value_encrypted` type.
    /// Content is the decrypted display value (or "***" for secrets).
    value_encrypted: String,
    is_secret: bool,
    created_at: chrono::DateTime<chrono::Utc>,
}

impl EnvVarResponse {
    fn from_env(env: ServiceEnv, secret_key: &str) -> Self {
        let display = if env.is_secret {
            "***".to_string()
        } else {
            shipyard_common::crypto::decrypt_or_passthrough(secret_key, &env.value_encrypted)
        };
        Self {
            id: env.id,
            service_id: env.service_id,
            key: env.key,
            value_encrypted: display,
            is_secret: env.is_secret,
            created_at: env.created_at,
        }
    }
}

#[derive(Debug, Deserialize)]
struct BulkEnvBody {
    envs: Vec<EnvVarRequest>,
}

#[derive(Debug, Deserialize, Default)]
struct ContainerLogsQuery {
    #[serde(default = "default_tail")]
    tail: u32,
    #[serde(default)]
    timestamps: bool,
}
fn default_tail() -> u32 { 100 }

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        // Service → deployment actions
        .route("/services/:id/deployments",                       get(list_deployments))
        .route("/services/:id/deploy",                            post(trigger_deploy))
        .route("/services/:id/deployments/:dep_id",               get(get_service_deployment))
        .route("/services/:id/deployments/:dep_id/steps",         get(list_service_dep_steps))
        .route("/services/:id/deployments/:dep_id/logs",          get(list_service_dep_logs))
        .route("/services/:id/deployments/:dep_id/cancel",        post(cancel_service_deployment))
        // Service control
        .route("/services/:id/stop",                              post(stop_service))
        .route("/services/:id/restart",                           post(restart_service))
        .route("/services/:id/redeploy",                          post(redeploy_service))
        // Deployment-only (look up service for auth)
        .route("/deployments/:dep_id/steps",                      get(list_dep_steps))
        .route("/deployments/:dep_id/logs",                       get(list_dep_logs))
        .route("/deployments/:dep_id/cancel",                     post(cancel_dep))
        // Env vars
        .route("/services/:id/env",                               get(list_env).post(upsert_env).put(bulk_env_put))
        .route("/services/:id/env/bulk",                          post(bulk_env_post))
        // Delete by row UUID (client passes the env row id, not the key string)
        .route("/services/:id/env/:env_id",                       delete(delete_env))
        // Container logs
        .route("/services/:id/containers/:cid/logs",              get(container_logs))
        .route("/services/:id/containers/:cid/logs/stream",       get(container_logs_stream))
        // Container stats (one-shot Docker stats snapshot)
        .route("/services/:id/containers/:cid/stats",             get(container_stats))
}

// ─── Deployment handlers (service-scoped) ────────────────────────────────────

async fn list_deployments(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Deployment>>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;

    let deployments = sqlx::query_as::<_, Deployment>(
        "SELECT id, service_id, triggered_by, source_ref, status::text AS status, created_at, finished_at
         FROM deployments WHERE service_id = $1
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(deployments)))
}

async fn trigger_deploy(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    body: Option<Json<DeployBody>>,
) -> Result<(StatusCode, Json<ApiResponse<Deployment>>), ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:deploy").await.map_err(ApiAppError)?;

    let source_ref = body.map(|b| b.0.source_ref).unwrap_or_else(default_ref);
    let triggered_by = auth.email.clone();
    let source_ref_log = source_ref.clone();

    let deployment_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         VALUES ($1, $2, $3, $4, 'running'::deployment_status, NOW())",
    )
    .bind(deployment_id)
    .bind(service_id)
    .bind(&triggered_by)
    .bind(&source_ref)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let engine = DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
        state.config.data_dir.clone(),
        state.config.static_server.retention_versions,
    );

    let deploy_notify = Arc::clone(&state.swarm_sync_trigger);
    tokio::spawn(async move {
        if let Err(e) = engine.deploy(deployment_id, service_id, &triggered_by, &source_ref).await {
            tracing::error!(deployment_id = %deployment_id, "Deploy error: {e}");
        }
        deploy_notify.notify_one();
    });

    crate::middleware::audit::write_audit_log(
        &state.db, &auth, "trigger_deployment",
        Some("deployment"), Some(deployment_id), None,
        Some(serde_json::json!({ "service_id": service_id, "source_ref": source_ref_log })),
    ).await;

    let deployment = sqlx::query_as::<_, Deployment>(
        "SELECT id, service_id, triggered_by, source_ref, status::text AS status, created_at, finished_at
         FROM deployments WHERE id = $1",
    )
    .bind(deployment_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Deployment not found after creation".to_string())))?;

    Ok((StatusCode::ACCEPTED, Json(ApiResponse::ok(deployment))))
}

async fn get_service_deployment(
    auth: AuthUser,
    Path((service_id, dep_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Deployment>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    let dep = fetch_deployment_for_service(&state.db, dep_id, service_id).await?;
    Ok(Json(ApiResponse::ok(dep)))
}

async fn list_service_dep_steps(
    auth: AuthUser,
    Path((service_id, dep_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<DeploymentStep>>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    let steps = fetch_steps(&state.db, dep_id).await?;
    Ok(Json(ApiResponse::ok(steps)))
}

async fn list_service_dep_logs(
    auth: AuthUser,
    Path((service_id, dep_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<ApiResponse<Vec<DeploymentLog>>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    let logs = fetch_logs(&state.db, dep_id, q.level, q.step_id).await?;
    Ok(Json(ApiResponse::ok(logs)))
}

async fn cancel_service_deployment(
    auth: AuthUser,
    Path((service_id, dep_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    do_cancel(&state.db, dep_id).await
}

// ─── Deployment-only handlers (no service_id in path) ────────────────────────

async fn list_dep_steps(
    auth: AuthUser,
    Path(dep_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<DeploymentStep>>>, ApiAppError> {
    let service_id = service_id_from_dep(&state.db, dep_id).await?;
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    let steps = fetch_steps(&state.db, dep_id).await?;
    Ok(Json(ApiResponse::ok(steps)))
}

async fn list_dep_logs(
    auth: AuthUser,
    Path(dep_id): Path<Uuid>,
    State(state): State<AppState>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<ApiResponse<Vec<DeploymentLog>>>, ApiAppError> {
    let service_id = service_id_from_dep(&state.db, dep_id).await?;
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    let logs = fetch_logs(&state.db, dep_id, q.level, q.step_id).await?;
    Ok(Json(ApiResponse::ok(logs)))
}

async fn cancel_dep(
    auth: AuthUser,
    Path(dep_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let service_id = service_id_from_dep(&state.db, dep_id).await?;
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    do_cancel(&state.db, dep_id).await
}

// ─── Service control ──────────────────────────────────────────────────────────

async fn stop_service(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:deploy").await.map_err(ApiAppError)?;

    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT type::text, COALESCE(directory_path, '') FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (svc_type, directory_path) = row.ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id)))
    })?;

    if svc_type == "docker_compose" {
        compose_stop(&directory_path, service_id).await;
        // Also stop child Swarm services (best-effort; they may not exist)
        let children: Vec<(Uuid,)> = sqlx::query_as(
            "SELECT id FROM services WHERE service_parent_id = $1",
        )
        .bind(service_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
        for (child_id,) in children {
            let name = docker_service_name(&state, child_id);
            let _ = state.docker.scale_service(&name, 0).await;
        }
        sqlx::query(
            "UPDATE services SET status = 'stopped', replicas = 0, updated_at = NOW()
             WHERE id = $1 OR service_parent_id = $1",
        )
        .bind(service_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    } else {
        let svc_name = docker_service_name(&state, service_id);
        state.docker.scale_service(&svc_name, 0).await.map_err(ApiAppError)?;
        sqlx::query(
            "UPDATE services SET status = 'stopped', replicas = 0, updated_at = NOW() WHERE id = $1",
        )
        .bind(service_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Service stopped", "replicas": 0 }))))
}

async fn restart_service(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:deploy").await.map_err(ApiAppError)?;

    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT type::text, COALESCE(directory_path, '') FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (svc_type, directory_path) = row.ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id)))
    })?;

    if svc_type == "docker_compose" {
        compose_stop(&directory_path, service_id).await;
        compose_start(&directory_path, service_id).await;
        sqlx::query(
            "UPDATE services SET status = 'running', updated_at = NOW()
             WHERE id = $1 OR service_parent_id = $1",
        )
        .bind(service_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
        Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Service restarted" }))))
    } else {
        let svc_name = docker_service_name(&state, service_id);

        // Load current env vars from DB and apply them to the running service
        // so that any vars added/changed since the last deploy take effect.
        let env_rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT key, value_encrypted
             FROM service_envs
             WHERE service_id = $1
               AND key NOT LIKE '\\_\\_%' ESCAPE '\\'
             ORDER BY key ASC",
        )
        .bind(service_id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let env_vars: Vec<String> = env_rows
            .into_iter()
            .map(|(k, v)| {
                let plain = shipyard_common::crypto::decrypt_or_passthrough(
                    &state.config.auth.secret_key, &v,
                );
                format!("{k}={plain}")
            })
            .collect();

        // Update the service spec with fresh env vars; this also bumps force_update
        // so Docker Swarm replaces the running tasks even when the image is unchanged.
        state.docker.apply_envs_to_service(&svc_name, env_vars).await.map_err(ApiAppError)?;

        sqlx::query(
            "UPDATE services SET status = 'running', replicas = 1, updated_at = NOW() WHERE id = $1",
        )
        .bind(service_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
        Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Service restarted", "replicas": 1 }))))
    }
}

/// Run `docker compose stop` for a compose-type service (best-effort).
async fn compose_stop(directory_path: &str, service_id: Uuid) {
    let candidates = [
        format!("{directory_path}/docker-compose.yml"),
        format!("{directory_path}/docker-compose.yaml"),
        format!("{directory_path}/.shipyard-compose.yml"),
        format!("{directory_path}/compose.yml"),
        format!("{directory_path}/compose.yaml"),
    ];
    let compose_file = candidates.iter().find(|p| std::path::Path::new(p).exists());
    if let Some(cf) = compose_file {
        match tokio::process::Command::new("docker")
            .args(["compose", "-f", cf, "stop"])
            .current_dir(directory_path)
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                tracing::info!(%service_id, "docker compose stop succeeded");
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::warn!(%service_id, "docker compose stop: {stderr}");
            }
            Err(e) => tracing::warn!(%service_id, "docker compose stop spawn failed: {e}"),
        }
    } else {
        tracing::warn!(%service_id, directory_path, "compose file not found for stop");
    }
}

/// Run `docker compose start` for a compose-type service (best-effort).
async fn compose_start(directory_path: &str, service_id: Uuid) {
    let candidates = [
        format!("{directory_path}/docker-compose.yml"),
        format!("{directory_path}/docker-compose.yaml"),
        format!("{directory_path}/.shipyard-compose.yml"),
        format!("{directory_path}/compose.yml"),
        format!("{directory_path}/compose.yaml"),
    ];
    let compose_file = candidates.iter().find(|p| std::path::Path::new(p).exists());
    if let Some(cf) = compose_file {
        match tokio::process::Command::new("docker")
            .args(["compose", "-f", cf, "start"])
            .current_dir(directory_path)
            .output()
            .await
        {
            Ok(out) if out.status.success() => {
                tracing::info!(%service_id, "docker compose start succeeded");
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                tracing::warn!(%service_id, "docker compose start: {stderr}");
            }
            Err(e) => tracing::warn!(%service_id, "docker compose start spawn failed: {e}"),
        }
    } else {
        tracing::warn!(%service_id, directory_path, "compose file not found for start");
    }
}

async fn redeploy_service(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ApiResponse<Deployment>>), ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:deploy").await.map_err(ApiAppError)?;

    let source_ref = "manual".to_string();
    let triggered_by = auth.email.clone();

    let deployment_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         VALUES ($1, $2, $3, $4, 'running'::deployment_status, NOW())",
    )
    .bind(deployment_id)
    .bind(service_id)
    .bind(&triggered_by)
    .bind(&source_ref)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let engine = DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
        state.config.data_dir.clone(),
        state.config.static_server.retention_versions,
    );

    let redeploy_notify = Arc::clone(&state.swarm_sync_trigger);
    tokio::spawn(async move {
        if let Err(e) = engine.deploy(deployment_id, service_id, &triggered_by, &source_ref).await {
            tracing::error!(deployment_id = %deployment_id, "Redeploy error: {e}");
        }
        redeploy_notify.notify_one();
    });

    let deployment = sqlx::query_as::<_, Deployment>(
        "SELECT id, service_id, triggered_by, source_ref, status::text AS status, created_at, finished_at
         FROM deployments WHERE id = $1",
    )
    .bind(deployment_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Deployment not found after creation".to_string())))?;

    Ok((StatusCode::ACCEPTED, Json(ApiResponse::ok(deployment))))
}

// ─── Env vars ─────────────────────────────────────────────────────────────────

async fn list_env(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<EnvVarResponse>>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;
    let envs = sqlx::query_as::<_, ServiceEnv>(
        "SELECT id, service_id, key, value_encrypted, is_secret, created_at
         FROM service_envs WHERE service_id = $1 ORDER BY key ASC",
    )
    .bind(service_id)
    .fetch_all(&state.db).await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    let key = &state.config.auth.secret_key;
    Ok(Json(ApiResponse::ok(envs.into_iter().map(|e| EnvVarResponse::from_env(e, key)).collect())))
}

async fn upsert_env(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<EnvVarRequest>,
) -> Result<(StatusCode, Json<ApiResponse<EnvVarResponse>>), ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:write").await.map_err(ApiAppError)?;
    if body.key.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("key is required".to_string())));
    }
    let sk = &state.config.auth.secret_key;
    let encrypted = shipyard_common::crypto::encrypt_or_passthrough(sk, &body.value);
    let env = sqlx::query_as::<_, ServiceEnv>(
        "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW())
         ON CONFLICT (service_id, key) DO UPDATE
           SET value_encrypted = EXCLUDED.value_encrypted, is_secret = EXCLUDED.is_secret
         RETURNING id, service_id, key, value_encrypted, is_secret, created_at",
    )
    .bind(Uuid::now_v7()).bind(service_id).bind(&body.key).bind(&encrypted).bind(body.is_secret)
    .fetch_one(&state.db).await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    publish_env_changed(&state, service_id).await;
    Ok((StatusCode::CREATED, Json(ApiResponse::ok(EnvVarResponse::from_env(env, sk)))))
}

async fn bulk_env_put(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(items): Json<Vec<EnvVarRequest>>,
) -> Result<Json<ApiResponse<Vec<EnvVarResponse>>>, ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:write").await.map_err(ApiAppError)?;
    apply_bulk(&state, service_id, items).await
}

async fn bulk_env_post(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<BulkEnvBody>,
) -> Result<Json<ApiResponse<Vec<EnvVarResponse>>>, ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:write").await.map_err(ApiAppError)?;
    apply_bulk(&state, service_id, body.envs).await
}

async fn apply_bulk(
    state: &AppState,
    service_id: Uuid,
    items: Vec<EnvVarRequest>,
) -> Result<Json<ApiResponse<Vec<EnvVarResponse>>>, ApiAppError> {
    let sk = state.config.auth.secret_key.clone();
    let mut results = Vec::new();
    for item in &items {
        if item.key.is_empty() { continue; }
        let encrypted = shipyard_common::crypto::encrypt_or_passthrough(&sk, &item.value);
        let env = sqlx::query_as::<_, ServiceEnv>(
            "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
             VALUES ($1, $2, $3, $4, $5, NOW())
             ON CONFLICT (service_id, key) DO UPDATE
               SET value_encrypted = EXCLUDED.value_encrypted, is_secret = EXCLUDED.is_secret
             RETURNING id, service_id, key, value_encrypted, is_secret, created_at",
        )
        .bind(Uuid::now_v7()).bind(service_id).bind(&item.key).bind(&encrypted).bind(item.is_secret)
        .fetch_one(&state.db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
        results.push(EnvVarResponse::from_env(env, &sk));
    }
    publish_env_changed(&state, service_id).await;
    Ok(Json(ApiResponse::ok(results)))
}

async fn delete_env(
    auth: AuthUser,
    Path((service_id, env_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_permission(&state.db, auth.user_id, service_id, "service:write").await.map_err(ApiAppError)?;
    let n = sqlx::query("DELETE FROM service_envs WHERE id = $1 AND service_id = $2")
        .bind(env_id).bind(service_id)
        .execute(&state.db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .rows_affected();
    if n == 0 {
        return Err(ApiAppError(AppError::NotFound(format!("Env var '{env_id}' not found"))));
    }
    publish_env_changed(&state, service_id).await;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Deleted" }))))
}

async fn publish_env_changed(state: &AppState, service_id: Uuid) {
    let Ok(Some((project_id, org_id))) = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT s.project_id, p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await else { return };

    crate::services::detect_and_store_platform_refs(
        &state.db, service_id, project_id, &state.config.auth.secret_key,
    ).await;

    let topic = shipyard_mqtt::topics::topology(org_id, project_id);
    state.mqtt.publish_status(&topic, &MqttPayload::new("service.env.changed")
        .with_meta(serde_json::json!({ "service_id": service_id }))).await.ok();
    state.mqtt.publish_status(&topic, &MqttPayload::new("topology.changed")
        .with_meta(serde_json::json!({ "reason": "env_ref" }))).await.ok();
}

// ─── Container stats types ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ContainerStatsSnapshot {
    cpu_percent: f64,
    memory_usage_bytes: u64,
    memory_limit_bytes: u64,
    memory_percent: f64,
    net_rx_bytes: u64,
    net_tx_bytes: u64,
    block_read_bytes: u64,
    block_write_bytes: u64,
    pids: u64,
    timestamp: String,
}

fn parse_stats_snapshot(v: &serde_json::Value) -> ContainerStatsSnapshot {
    // CPU % — Docker provides cpu_stats (current) and precpu_stats (previous 1s sample)
    let cpu_total    = v["cpu_stats"]["cpu_usage"]["total_usage"].as_u64().unwrap_or(0);
    let precpu_total = v["precpu_stats"]["cpu_usage"]["total_usage"].as_u64().unwrap_or(0);
    let sys_cpu      = v["cpu_stats"]["system_cpu_usage"].as_u64().unwrap_or(0);
    let presys_cpu   = v["precpu_stats"]["system_cpu_usage"].as_u64().unwrap_or(0);
    let num_cpus     = v["cpu_stats"]["online_cpus"].as_u64().unwrap_or(1).max(1);

    let cpu_delta    = cpu_total.saturating_sub(precpu_total);
    let system_delta = sys_cpu.saturating_sub(presys_cpu);
    let cpu_percent  = if system_delta > 0 {
        ((cpu_delta as f64 / system_delta as f64) * num_cpus as f64 * 100.0)
            .clamp(0.0, 100.0 * num_cpus as f64)
    } else {
        0.0
    };

    // Memory (subtract page cache for resident-only usage)
    let mem_usage = v["memory_stats"]["usage"].as_u64().unwrap_or(0);
    let mem_cache = v["memory_stats"]["stats"]["cache"].as_u64()
        .or_else(|| v["memory_stats"]["stats"]["inactive_file"].as_u64())
        .unwrap_or(0);
    let mem_limit     = v["memory_stats"]["limit"].as_u64().unwrap_or(1).max(1);
    let mem_net       = mem_usage.saturating_sub(mem_cache);
    let mem_percent   = ((mem_net as f64 / mem_limit as f64) * 100.0).clamp(0.0, 100.0);

    // Network I/O (cumulative totals across all interfaces)
    let (mut net_rx, mut net_tx) = (0u64, 0u64);
    if let Some(networks) = v["networks"].as_object() {
        for iface in networks.values() {
            net_rx += iface["rx_bytes"].as_u64().unwrap_or(0);
            net_tx += iface["tx_bytes"].as_u64().unwrap_or(0);
        }
    }

    // Block I/O (cumulative totals)
    let (mut blk_read, mut blk_write) = (0u64, 0u64);
    if let Some(ios) = v["blkio_stats"]["io_service_bytes_recursive"].as_array() {
        for io in ios {
            match io["op"].as_str() {
                Some("Read")  => blk_read  += io["value"].as_u64().unwrap_or(0),
                Some("Write") => blk_write += io["value"].as_u64().unwrap_or(0),
                _ => {}
            }
        }
    }

    let r2 = |x: f64| (x * 100.0).round() / 100.0;

    ContainerStatsSnapshot {
        cpu_percent:        r2(cpu_percent),
        memory_usage_bytes: mem_net,
        memory_limit_bytes: mem_limit,
        memory_percent:     r2(mem_percent),
        net_rx_bytes:       net_rx,
        net_tx_bytes:       net_tx,
        block_read_bytes:   blk_read,
        block_write_bytes:  blk_write,
        pids:               v["pids_stats"]["current"].as_u64().unwrap_or(0),
        timestamp:          chrono::Utc::now().to_rfc3339(),
    }
}

// ─── Container stats SSE stream ───────────────────────────────────────────────

/// GET /services/:id/containers/:cid/stats
///
/// SSE stream: Docker stats with `stream=true` pushed as one `ContainerStatsSnapshot`
/// JSON per second.  Each event already contains the two-sample CPU delta
/// (`cpu_stats` / `precpu_stats`) so the backend can compute CPU % per event
/// without keeping state.  Network/block IO are cumulative totals; the frontend
/// computes per-second deltas between consecutive events.
async fn container_stats(
    auth: AuthUser,
    Path((service_id, container_docker_id)): Path<(Uuid, String)>,
    State(state): State<AppState>,
) -> Sse<ReceiverStream<Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    let auth_ok  = require_service_access(&state.db, auth.user_id, service_id).await;

    tokio::spawn(async move {
        if let Err(e) = auth_ok {
            let _ = tx.send(Ok(Event::default().event("error").data(e.to_string()))).await;
            return;
        }

        // Remote containers can't be stats'd via local Docker socket.
        if is_remote_container(&state, service_id, &container_docker_id).await {
            let _ = tx.send(Ok(Event::default().event("remote")
                .data("Container is running on a remote Swarm node — live stats unavailable")))
                .await;
            return;
        }

        use tokio::net::UnixStream;

        let mut stream = match UnixStream::connect("/var/run/docker.sock").await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(Ok(Event::default().event("error")
                        .data(format!("Cannot connect to Docker socket: {e}"))))
                    .await;
                return;
            }
        };

        let request = format!(
            "GET /v1.45/containers/{}/stats?stream=true HTTP/1.1\r\n\
             Host: localhost\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             \r\n",
            urlencoding::encode(&container_docker_id)
        );

        if let Err(e) = stream.write_all(request.as_bytes()).await {
            let _ = tx
                .send(Ok(Event::default().event("error").data(format!("Write failed: {e}"))))
                .await;
            return;
        }

        let mut reader = BufReader::new(stream);
        let mut line   = String::new();
        let mut chunked = false;
        let mut status_ok = true;

        // Parse HTTP response header block
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            let lower = line.to_ascii_lowercase();
            if lower.starts_with("http/") && !lower.contains(" 200") {
                status_ok = false;
            }
            if lower.contains("transfer-encoding: chunked") {
                chunked = true;
            }
            if line == "\r\n" || line == "\n" {
                break;
            }
        }

        if !status_ok {
            // Container not found locally — mark it as orphan in DB so the UI cleans up.
            let _ = sqlx::query(
                "UPDATE containers SET status = 'orphan' \
                 WHERE docker_container_id = $1 AND service_id = $2 \
                   AND status NOT IN ('orphan','failed','shutdown','complete','rejected')"
            )
            .bind(&container_docker_id)
            .bind(service_id)
            .execute(&state.db)
            .await;

            let _ = tx
                .send(Ok(Event::default().event("error")
                    .data(format!("Container {container_docker_id} not found — record marked as orphan"))))
                .await;
            return;
        }

        // Stream stats events — one JSON object per ~1s Docker interval
        loop {
            let json_bytes: Vec<u8> = if chunked {
                // Read chunk-size line (hex)
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let size = match usize::from_str_radix(line.trim(), 16) {
                    Ok(s) => s,
                    Err(_) => break,
                };
                if size == 0 { break; }

                // Read exactly `size` bytes of chunk data
                let mut buf = vec![0u8; size];
                if reader.read_exact(&mut buf).await.is_err() { break; }

                // Consume the trailing CRLF that follows every chunk
                line.clear();
                let _ = reader.read_line(&mut line).await;
                buf
            } else {
                // Bare ndjson (no chunked encoding)
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                line.trim().as_bytes().to_vec()
            };

            if json_bytes.is_empty() { continue; }

            let v: serde_json::Value = match serde_json::from_slice(&json_bytes) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let snapshot = parse_stats_snapshot(&v);
            let data     = match serde_json::to_string(&snapshot) {
                Ok(d) => d,
                Err(_) => continue,
            };

            if tx.send(Ok(Event::default().data(data))).await.is_err() {
                break; // client disconnected
            }
        }
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

// ─── Container logs handler ───────────────────────────────────────────────────

async fn container_logs(
    auth: AuthUser,
    Path((service_id, container_docker_id)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    Query(q): Query<ContainerLogsQuery>,
) -> Result<Json<ApiResponse<Vec<String>>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;

    if is_remote_container(&state, service_id, &container_docker_id).await {
        // Container is on a worker node — use `docker service logs` (cross-node via manager)
        let task_id = resolve_task_id(&state.db, service_id, &container_docker_id).await;
        let target = task_id.unwrap_or_else(|| docker_service_name(&state, service_id));
        let tail_arg = format!("--tail={}", q.tail.min(5000));
        let output = tokio::process::Command::new("docker")
            .args(["service", "logs", "--raw", &tail_arg, &target])
            .output()
            .await
            .map_err(|e| ApiAppError(AppError::Internal(format!("docker service logs failed: {e}"))))?;
        let text = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<String> = text.lines().map(String::from).collect();
        return Ok(Json(ApiResponse::ok(lines)));
    }

    use shipyard_docker::types::LogOpts;
    let lines = state
        .docker
        .container_logs(
            &container_docker_id,
            LogOpts {
                stdout: true,
                stderr: true,
                follow: false,
                since: None,
                until: None,
                timestamps: q.timestamps,
                tail: Some(q.tail.to_string()),
            },
        )
        .await
        .map_err(ApiAppError)?;
    Ok(Json(ApiResponse::ok(lines)))
}

/// GET /services/:id/containers/:cid/logs/stream — real-time SSE log stream
async fn container_logs_stream(
    auth: AuthUser,
    Path((service_id, container_id)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    Query(q): Query<ContainerLogsQuery>,
) -> Sse<ReceiverStream<Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(128);
    let tail = q.tail.min(5000u32);

    // Auth check before handing off to the background task
    let auth_result = require_service_access(&state.db, auth.user_id, service_id).await;

    tokio::spawn(async move {
        if let Err(e) = auth_result {
            let _ = tx.send(Ok(Event::default().event("error").data(e.to_string()))).await;
            return;
        }

        let tail_arg = format!("--tail={}", tail);

        // Choose docker logs vs docker service logs based on whether the container
        // is on the local (manager) node or a remote worker node.
        let (cmd_args, use_service_logs) =
            if is_remote_container(&state, service_id, &container_id).await {
                let target = resolve_task_id(&state.db, service_id, &container_id)
                    .await
                    .unwrap_or_else(|| docker_service_name(&state, service_id));
                (vec!["service".to_string(), "logs".to_string(), "--raw".to_string(),
                      "-f".to_string(), tail_arg, target], true)
            } else {
                let mut args = vec!["logs".to_string(), "-f".to_string(), tail_arg];
                if q.timestamps { args.push("--timestamps".to_string()); }
                args.push(container_id);
                (args, false)
            };
        let _ = use_service_logs; // consumed via cmd_args

        let mut child = match tokio::process::Command::new("docker")
            .args(&cmd_args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(Ok(Event::default().event("error").data(format!("Failed to start docker logs: {e}"))))
                    .await;
                return;
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let tx_out = tx.clone();
        let tx_err = tx.clone();

        let h_out = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_out.send(Ok(Event::default().data(line))).await.is_err() { break; }
            }
        });

        let h_err = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_err.send(Ok(Event::default().data(line))).await.is_err() { break; }
            }
        });

        let _ = tokio::join!(h_out, h_err);
        let _ = child.kill().await;
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

// ─── Shared DB helpers ────────────────────────────────────────────────────────

fn docker_service_name(state: &AppState, service_id: Uuid) -> String {
    format!("{}-{}", state.config.docker.label_prefix, service_id)
}

/// Returns true if the container is running on a remote Swarm worker node.
/// Falls back to false (treat as local) when swarm info is unavailable or the
/// container has no node_id recorded.
async fn is_remote_container(state: &AppState, service_id: Uuid, docker_container_id: &str) -> bool {
    let local_node_id = match state.docker.swarm_info().await {
        Ok(i) if !i.node_id.is_empty() => i.node_id,
        _ => return false,
    };

    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT node_id FROM containers WHERE docker_container_id = $1 AND service_id = $2",
    )
    .bind(docker_container_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None);

    matches!(row, Some((Some(ref n),)) if !n.is_empty() && n != &local_node_id)
}

/// Looks up the Swarm task ID for a container — used to target `docker service logs <task_id>`.
/// Falls back to None if the container has no task ID recorded.
async fn resolve_task_id(
    db: &sqlx::PgPool,
    service_id: Uuid,
    docker_container_id: &str,
) -> Option<String> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT docker_task_id FROM containers WHERE docker_container_id = $1 AND service_id = $2",
    )
    .bind(docker_container_id)
    .bind(service_id)
    .fetch_optional(db)
    .await
    .unwrap_or(None);

    row.and_then(|(tid,)| tid).filter(|s| !s.is_empty())
}

/// Look up service_id for a deployment (for auth without a service path param).
async fn service_id_from_dep(db: &sqlx::PgPool, dep_id: Uuid) -> Result<Uuid, ApiAppError> {
    sqlx::query_as::<_, (Uuid,)>("SELECT service_id FROM deployments WHERE id = $1")
        .bind(dep_id)
        .fetch_optional(db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .map(|(id,)| id)
        .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Deployment '{dep_id}' not found"))))
}

async fn fetch_deployment_for_service(
    db: &sqlx::PgPool,
    dep_id: Uuid,
    service_id: Uuid,
) -> Result<Deployment, ApiAppError> {
    sqlx::query_as::<_, Deployment>(
        "SELECT id, service_id, triggered_by, source_ref, status::text AS status, created_at, finished_at
         FROM deployments WHERE id = $1 AND service_id = $2",
    )
    .bind(dep_id).bind(service_id)
    .fetch_optional(db).await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Deployment '{dep_id}' not found"))))
}

async fn fetch_steps(db: &sqlx::PgPool, dep_id: Uuid) -> Result<Vec<DeploymentStep>, ApiAppError> {
    sqlx::query_as::<_, DeploymentStep>(
        "SELECT id, deployment_id, name, status::text AS status, order_index, started_at, finished_at
         FROM deployment_steps WHERE deployment_id = $1 ORDER BY order_index ASC",
    )
    .bind(dep_id)
    .fetch_all(db).await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))
}

async fn fetch_logs(
    db: &sqlx::PgPool,
    dep_id: Uuid,
    level: Option<String>,
    step_id: Option<Uuid>,
) -> Result<Vec<DeploymentLog>, ApiAppError> {
    sqlx::query_as::<_, DeploymentLog>(
        "SELECT id, deployment_id, step_id, level::text AS level, message, timestamp
         FROM deployment_logs
         WHERE deployment_id = $1
           AND ($2::text IS NULL OR level::text = $2)
           AND ($3::uuid IS NULL OR step_id = $3)
         ORDER BY timestamp ASC LIMIT 500",
    )
    .bind(dep_id).bind(level).bind(step_id)
    .fetch_all(db).await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))
}

async fn do_cancel(db: &sqlx::PgPool, dep_id: Uuid) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT status::text FROM deployments WHERE id = $1",
    )
    .bind(dep_id)
    .fetch_optional(db).await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Deployment '{dep_id}' not found"))))?;

    let status = row.0;
    if matches!(status.as_str(), "success" | "failed" | "cancelled") {
        return Err(ApiAppError(AppError::Conflict(format!("Already in terminal state '{status}'"))));
    }

    sqlx::query("UPDATE deployments SET status = 'cancelled', finished_at = NOW() WHERE id = $1")
        .bind(dep_id).execute(db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    sqlx::query(
        "UPDATE deployment_steps SET status = 'failed', finished_at = NOW()
         WHERE deployment_id = $1 AND status IN ('pending', 'running')",
    )
    .bind(dep_id).execute(db).await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Deployment cancelled" }))))
}
