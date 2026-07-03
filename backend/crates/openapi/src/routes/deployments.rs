use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{ApiKeyUser, SCOPE_DEPLOY, SCOPE_READ},
    error::OpenApiError,
    response::{OkResponse, PageParams, PageResponse},
    OpenApiState,
};

pub fn routes() -> Router<OpenApiState> {
    Router::new()
        .route("/services/:service_id/deploy", post(trigger_deploy))
        .route("/services/:service_id/deployments", get(list_deployments))
        .route("/deployments/:deployment_id", get(get_deployment))
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub deployment_id: Uuid,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct DeploymentResponse {
    pub id: Uuid,
    pub service_id: Uuid,
    pub triggered_by: String,
    pub source_ref: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct TriggerRequest {
    #[serde(default)]
    pub source_ref: String,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// POST /openapi/v1/services/:service_id/deploy
async fn trigger_deploy(
    caller: ApiKeyUser,
    Path(service_id): Path<Uuid>,
    State(state): State<OpenApiState>,
    Json(body): Json<TriggerRequest>,
) -> Result<(StatusCode, Json<OkResponse<TriggerResponse>>), OpenApiError> {
    caller.require_scope(SCOPE_DEPLOY)?;

    // Ensure the service is in the caller's org
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT s.id FROM services s
         JOIN projects p ON p.id = s.project_id
         WHERE s.id = $1 AND p.org_id = $2",
    )
    .bind(service_id)
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| OpenApiError::Database(e.to_string()))?;

    if exists.is_none() {
        return Err(OpenApiError::NotFound(format!("Service '{}' not found", service_id)));
    }

    let triggered_by = format!("api-key:{}", caller.key_name);

    // Parallelism gate — mirror the logic in the internal API
    let max_parallel = sqlx::query_as::<_, (String,)>(
        "SELECT value::text FROM system_config WHERE key = 'max_parallel_deployments'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .and_then(|(v,)| v.trim_matches('"').parse::<i64>().ok())
    .unwrap_or(0);

    if max_parallel > 0 {
        let running: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM deployments WHERE status = 'running'::deployment_status",
        )
        .fetch_one(&state.db)
        .await
        .map_err(|e| OpenApiError::Database(e.to_string()))?;

        if running.0 >= max_parallel {
            let deployment_id = Uuid::new_v4();
            sqlx::query(
                "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
                 VALUES ($1, $2, $3, $4, 'queued'::deployment_status, NOW())",
            )
            .bind(deployment_id)
            .bind(service_id)
            .bind(&triggered_by)
            .bind(&body.source_ref)
            .execute(&state.db)
            .await
            .map_err(|e| OpenApiError::Database(e.to_string()))?;

            return Ok((
                StatusCode::ACCEPTED,
                Json(OkResponse::new(TriggerResponse {
                    deployment_id,
                    status: "queued".into(),
                })),
            ));
        }
    }

    let deployment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         VALUES ($1, $2, $3, $4, 'running'::deployment_status, NOW())",
    )
    .bind(deployment_id)
    .bind(service_id)
    .bind(&triggered_by)
    .bind(&body.source_ref)
    .execute(&state.db)
    .await
    .map_err(|e| OpenApiError::Database(e.to_string()))?;

    let engine = shipyard_engine::DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
    );

    let source_ref = body.source_ref.clone();
    tokio::spawn(async move {
        if let Err(e) = engine.deploy(deployment_id, service_id, &triggered_by, &source_ref).await {
            tracing::error!(deployment_id = %deployment_id, "Open API deployment error: {e}");
        }
    });

    tracing::info!(
        deployment_id = %deployment_id,
        service_id    = %service_id,
        key_id        = %caller.key_id,
        "Deployment triggered via Open API"
    );

    Ok((
        StatusCode::ACCEPTED,
        Json(OkResponse::new(TriggerResponse {
            deployment_id,
            status: "running".into(),
        })),
    ))
}

/// GET /openapi/v1/services/:service_id/deployments
async fn list_deployments(
    caller: ApiKeyUser,
    Path(service_id): Path<Uuid>,
    Query(page): Query<PageParams>,
    State(state): State<OpenApiState>,
) -> Result<Json<PageResponse<DeploymentResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    // Ensure service is in caller's org
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT s.id FROM services s
         JOIN projects p ON p.id = s.project_id
         WHERE s.id = $1 AND p.org_id = $2",
    )
    .bind(service_id)
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(OpenApiError::NotFound(format!("Service '{}' not found", service_id)));
    }

    let total: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM deployments WHERE service_id = $1")
            .bind(service_id)
            .fetch_one(&state.db)
            .await?;

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT id, service_id, triggered_by, source_ref, status::text,
                created_at, finished_at
         FROM deployments
         WHERE service_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(service_id)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&state.db)
    .await?;

    let deployments = rows
        .into_iter()
        .map(|(id, service_id, triggered_by, source_ref, status, created_at, finished_at)| {
            DeploymentResponse { id, service_id, triggered_by, source_ref, status, created_at, finished_at }
        })
        .collect();

    Ok(Json(PageResponse::new(deployments, total.0, page.page, page.per_page)))
}

/// GET /openapi/v1/deployments/:deployment_id
async fn get_deployment(
    caller: ApiKeyUser,
    Path(deployment_id): Path<Uuid>,
    State(state): State<OpenApiState>,
) -> Result<Json<OkResponse<DeploymentResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT d.id, d.service_id, d.triggered_by, d.source_ref, d.status::text,
                d.created_at, d.finished_at
         FROM deployments d
         JOIN services s  ON s.id = d.service_id
         JOIN projects p  ON p.id = s.project_id
         WHERE d.id = $1 AND p.org_id = $2",
    )
    .bind(deployment_id)
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OpenApiError::NotFound(format!("Deployment '{}' not found", deployment_id)))?;

    let (id, service_id, triggered_by, source_ref, status, created_at, finished_at) = row;
    Ok(Json(OkResponse::new(DeploymentResponse {
        id, service_id, triggered_by, source_ref, status, created_at, finished_at,
    })))
}
