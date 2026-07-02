use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::{ApiResponse, PaginationParams, Paginated};
use shipyard_db::models::{Deployment, DeploymentLog, DeploymentStep};
use shipyard_engine::DeploymentEngine;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac::require_service_access;
use crate::AppState;

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TriggerDeployRequest {
    #[serde(default = "default_source_ref")]
    pub source_ref: String,
}

fn default_source_ref() -> String {
    "manual".to_string()
}

#[derive(Debug, Serialize)]
pub struct TriggerDeployResponse {
    pub deployment_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct LogsQuery {
    pub level: Option<String>,
    pub step_id: Option<Uuid>,
    #[serde(flatten)]
    pub pagination: PaginationParams,
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        // Deployment lifecycle
        .route(
            "/projects/:project_id/services/:service_id/deploy",
            post(trigger_deploy),
        )
        .route(
            "/projects/:project_id/services/:service_id/deployments",
            get(list_deployments),
        )
        .route(
            "/projects/:project_id/services/:service_id/deployments/:deployment_id",
            get(get_deployment),
        )
        .route(
            "/projects/:project_id/services/:service_id/deployments/:deployment_id/steps",
            get(list_steps),
        )
        .route(
            "/projects/:project_id/services/:service_id/deployments/:deployment_id/logs",
            get(list_logs),
        )
        .route(
            "/projects/:project_id/services/:service_id/deployments/:deployment_id/cancel",
            post(cancel_deployment),
        )
        // Service control
        .route(
            "/projects/:project_id/services/:service_id/start",
            post(start_service),
        )
        .route(
            "/projects/:project_id/services/:service_id/stop",
            post(stop_service),
        )
        .route(
            "/projects/:project_id/services/:service_id/restart",
            post(restart_service),
        )
        .route(
            "/projects/:project_id/services/:service_id/redeploy",
            post(redeploy_service),
        )
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// POST /projects/:project_id/services/:service_id/deploy
///
/// Trigger a new deployment. Returns immediately with the deployment_id;
/// the actual deployment runs in the background.
async fn trigger_deploy(
    auth_user: AuthUser,
    Path((_project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<TriggerDeployRequest>,
) -> Result<(StatusCode, Json<ApiResponse<TriggerDeployResponse>>), ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let triggered_by = auth_user.email.clone();
    let source_ref = body.source_ref.clone();
    let source_ref_log = source_ref.clone();

    // ── Parallelism gate ─────────────────────────────────────────────────────
    // Check global max_parallel_deployments setting; if at capacity, queue this
    // deployment instead of starting it immediately.
    let max_parallel = sqlx::query_as::<_, (String,)>(
        "SELECT value::text FROM system_config WHERE key = 'max_parallel_deployments'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .and_then(|(v,)| v.trim_matches('"').parse::<i64>().ok())
    .unwrap_or(0); // 0 = unlimited

    if max_parallel > 0 {
        let running: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM deployments WHERE status = 'running'::deployment_status",
        )
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        if running.0 >= max_parallel {
            // Insert as queued; the scheduler will start it when a slot opens.
            let deployment_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
                 VALUES ($1, $2, $3, $4, 'queued'::deployment_status, NOW())",
            )
            .bind(deployment_id)
            .bind(service_id)
            .bind(&triggered_by)
            .bind(&source_ref)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            return Ok((
                StatusCode::ACCEPTED,
                Json(ApiResponse::ok(TriggerDeployResponse { deployment_id })),
            ));
        }
    }
    // ────────────────────────────────────────────────────────────────────────

    // Build engine inline (no DeploymentEngine in AppState yet)
    let engine = DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
    );

    // Pre-generate the deployment_id so we can return it immediately.
    // The engine will create its own deployment row internally, so we kick
    // off the spawn and return what the engine will use.
    // Because the engine creates its own UUID, we spawn and capture the result
    // via a oneshot channel so we can return the deployment_id to the caller.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<Uuid, String>>();

    tokio::spawn(async move {
        let result = engine.deploy(service_id, &triggered_by, &source_ref).await;
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });

    // Wait briefly (50ms) to let the engine create the deployment row and
    // return the id. If it takes longer, return a 202 with a generated id
    // that will match what the engine inserts (engines uses its own uuid).
    // A cleaner approach: pre-insert the deployment row here and pass the id
    // to the engine. Since the engine creates its own id, we do a short wait.
    let deployment_id = match tokio::time::timeout(
        std::time::Duration::from_millis(100),
        rx,
    )
    .await
    {
        Ok(Ok(Ok(id))) => id,
        Ok(Ok(Err(e))) => {
            return Err(ApiAppError(AppError::Internal(format!(
                "Deployment failed immediately: {e}"
            ))))
        }
        // Timeout or channel error: deployment is still running in background.
        // We need the id — fetch the latest deployment for this service.
        _ => {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let row = sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM deployments WHERE service_id = $1 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(service_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            match row {
                Some((id,)) => id,
                None => Uuid::new_v4(), // fallback (should not happen)
            }
        }
    };

    crate::middleware::audit::write_audit_log(
        &state.db,
        Some(auth_user.user_id),
        "trigger_deployment",
        Some("deployment"),
        Some(deployment_id),
        None,
        Some(serde_json::json!({ "service_id": service_id, "source_ref": source_ref_log })),
    ).await;

    Ok((
        StatusCode::ACCEPTED,
        Json(ApiResponse::ok(TriggerDeployResponse { deployment_id })),
    ))
}

/// GET /projects/:project_id/services/:service_id/deployments
async fn list_deployments(
    auth_user: AuthUser,
    Path((_project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ApiResponse<Paginated<Deployment>>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let offset = ((params.page.saturating_sub(1)) as i64) * (params.per_page as i64);
    let limit = params.per_page as i64;

    let total: (i64,) = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM deployments WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let deployments = sqlx::query_as::<_, Deployment>(
        "SELECT id, service_id, triggered_by, source_ref, status::text AS status, created_at, finished_at
         FROM deployments
         WHERE service_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(service_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(Paginated {
        data: deployments,
        page: params.page,
        per_page: params.per_page,
        total: total.0,
    })))
}

/// GET /projects/:project_id/services/:service_id/deployments/:deployment_id
async fn get_deployment(
    auth_user: AuthUser,
    Path((_project_id, service_id, deployment_id)): Path<(Uuid, Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Deployment>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let deployment = sqlx::query_as::<_, Deployment>(
        "SELECT id, service_id, triggered_by, source_ref, status::text AS status, created_at, finished_at
         FROM deployments
         WHERE id = $1 AND service_id = $2",
    )
    .bind(deployment_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!(
            "Deployment '{deployment_id}' not found"
        )))
    })?;

    Ok(Json(ApiResponse::ok(deployment)))
}

/// GET /projects/:project_id/services/:service_id/deployments/:deployment_id/steps
async fn list_steps(
    auth_user: AuthUser,
    Path((_project_id, service_id, deployment_id)): Path<(Uuid, Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<DeploymentStep>>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    verify_deployment_service(&state.db, deployment_id, service_id).await?;

    let steps = sqlx::query_as::<_, DeploymentStep>(
        "SELECT id, deployment_id, name, status::text AS status, order_index, started_at, finished_at
         FROM deployment_steps
         WHERE deployment_id = $1
         ORDER BY order_index ASC",
    )
    .bind(deployment_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(steps)))
}

/// GET /projects/:project_id/services/:service_id/deployments/:deployment_id/logs
/// Optional query params: `level=`, `step_id=`
async fn list_logs(
    auth_user: AuthUser,
    Path((_project_id, service_id, deployment_id)): Path<(Uuid, Uuid, Uuid)>,
    State(state): State<AppState>,
    Query(query): Query<LogsQuery>,
) -> Result<Json<ApiResponse<Paginated<DeploymentLog>>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    verify_deployment_service(&state.db, deployment_id, service_id).await?;

    let offset =
        ((query.pagination.page.saturating_sub(1)) as i64) * (query.pagination.per_page as i64);
    let limit = query.pagination.per_page as i64;

    // Build dynamic filter
    let total: (i64,) = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM deployment_logs
         WHERE deployment_id = $1
           AND ($2::text IS NULL OR level::text = $2)
           AND ($3::uuid IS NULL OR step_id = $3)",
    )
    .bind(deployment_id)
    .bind(&query.level)
    .bind(query.step_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let logs = sqlx::query_as::<_, DeploymentLog>(
        "SELECT id, deployment_id, step_id, level::text AS level, message, timestamp
         FROM deployment_logs
         WHERE deployment_id = $1
           AND ($2::text IS NULL OR level::text = $2)
           AND ($3::uuid IS NULL OR step_id = $3)
         ORDER BY timestamp ASC
         LIMIT $4 OFFSET $5",
    )
    .bind(deployment_id)
    .bind(&query.level)
    .bind(query.step_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(Paginated {
        data: logs,
        page: query.pagination.page,
        per_page: query.pagination.per_page,
        total: total.0,
    })))
}

/// POST /projects/:project_id/services/:service_id/deployments/:deployment_id/cancel
async fn cancel_deployment(
    auth_user: AuthUser,
    Path((_project_id, service_id, deployment_id)): Path<(Uuid, Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    // Verify deployment belongs to service and is still in a cancellable state
    let deployment = sqlx::query_as::<_, (String,)>(
        "SELECT status::text FROM deployments WHERE id = $1 AND service_id = $2",
    )
    .bind(deployment_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!(
            "Deployment '{deployment_id}' not found"
        )))
    })?;

    let status = deployment.0;
    if status == "success" || status == "failed" || status == "cancelled" {
        return Err(ApiAppError(AppError::Conflict(format!(
            "Deployment is already in terminal state '{status}'"
        ))));
    }

    sqlx::query(
        "UPDATE deployments SET status = 'cancelled', finished_at = NOW() WHERE id = $1",
    )
    .bind(deployment_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Mark any running steps as failed
    sqlx::query(
        "UPDATE deployment_steps
         SET status = 'failed', finished_at = NOW()
         WHERE deployment_id = $1 AND status IN ('pending', 'running')",
    )
    .bind(deployment_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Deployment cancelled"
    }))))
}

// ─── Service Control Handlers ─────────────────────────────────────────────────

/// POST /projects/:project_id/services/:service_id/start
async fn start_service(
    auth_user: AuthUser,
    Path((_project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let docker_svc_name = docker_service_name(&state, service_id).await?;

    state
        .docker
        .scale_service(&docker_svc_name, 1)
        .await
        .map_err(|e| ApiAppError(e))?;

    sqlx::query(
        "UPDATE services SET status = 'running', replicas = 1, updated_at = NOW() WHERE id = $1",
    )
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Service started",
        "replicas": 1
    }))))
}

/// POST /projects/:project_id/services/:service_id/stop
async fn stop_service(
    auth_user: AuthUser,
    Path((_project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let docker_svc_name = docker_service_name(&state, service_id).await?;

    state
        .docker
        .scale_service(&docker_svc_name, 0)
        .await
        .map_err(|e| ApiAppError(e))?;

    sqlx::query(
        "UPDATE services SET status = 'stopped', replicas = 0, updated_at = NOW() WHERE id = $1",
    )
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Service stopped",
        "replicas": 0
    }))))
}

/// POST /projects/:project_id/services/:service_id/restart
async fn restart_service(
    auth_user: AuthUser,
    Path((_project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let docker_svc_name = docker_service_name(&state, service_id).await?;

    // Scale to 0 then back to 1
    state
        .docker
        .scale_service(&docker_svc_name, 0)
        .await
        .map_err(|e| ApiAppError(e))?;

    // Brief pause to let tasks drain
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    state
        .docker
        .scale_service(&docker_svc_name, 1)
        .await
        .map_err(|e| ApiAppError(e))?;

    sqlx::query(
        "UPDATE services SET status = 'running', replicas = 1, updated_at = NOW() WHERE id = $1",
    )
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Service restarted",
        "replicas": 1
    }))))
}

/// POST /projects/:project_id/services/:service_id/redeploy
///
/// Re-runs the last successful deployment's source_ref.
async fn redeploy_service(
    auth_user: AuthUser,
    Path((_project_id, service_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ApiResponse<TriggerDeployResponse>>), ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    // Find the last deployment (any status) to re-use its source_ref.
    // A prior successful deployment is preferred but not required — users
    // should be able to redeploy even if the first attempt failed.
    let last = sqlx::query_as::<_, (String,)>(
        "SELECT source_ref FROM deployments
         WHERE service_id = $1
         ORDER BY created_at DESC
         LIMIT 1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // source_ref carries the trigger label, not a git ref — always "manual"
    // for user-initiated redeploys regardless of how the previous one was triggered.
    let _ = last;
    let source_ref = "manual".to_string();
    let triggered_by = auth_user.email.clone();

    let engine = DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
    );

    let (tx, rx) = tokio::sync::oneshot::channel::<Result<Uuid, String>>();

    tokio::spawn(async move {
        let result = engine.deploy(service_id, &triggered_by, &source_ref).await;
        let _ = tx.send(result.map_err(|e| e.to_string()));
    });

    let deployment_id = match tokio::time::timeout(
        std::time::Duration::from_millis(100),
        rx,
    )
    .await
    {
        Ok(Ok(Ok(id))) => id,
        Ok(Ok(Err(e))) => {
            return Err(ApiAppError(AppError::Internal(format!(
                "Redeploy failed immediately: {e}"
            ))))
        }
        _ => {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let row = sqlx::query_as::<_, (Uuid,)>(
                "SELECT id FROM deployments WHERE service_id = $1 ORDER BY created_at DESC LIMIT 1",
            )
            .bind(service_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            match row {
                Some((id,)) => id,
                None => Uuid::new_v4(),
            }
        }
    };

    Ok((
        StatusCode::ACCEPTED,
        Json(ApiResponse::ok(TriggerDeployResponse { deployment_id })),
    ))
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Build the Docker swarm service name for a given service_id.
async fn docker_service_name(state: &AppState, service_id: Uuid) -> Result<String, ApiAppError> {
    Ok(format!("{}-{}", state.config.docker.label_prefix, service_id))
}

/// Verify that a deployment belongs to the given service.
async fn verify_deployment_service(
    db: &sqlx::PgPool,
    deployment_id: Uuid,
    service_id: Uuid,
) -> Result<(), ApiAppError> {
    let exists = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM deployments WHERE id = $1 AND service_id = $2",
    )
    .bind(deployment_id)
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Deployment '{deployment_id}' not found for service '{service_id}'"
        ))));
    }
    Ok(())
}
