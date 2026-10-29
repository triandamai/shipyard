use std::convert::Infallible;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_observability::{
    LogPage, LogQuery as ObsLogQuery, LogQueryParams, LogStatsParams, LogStatsResult,
    LogStore, PgLogStore, Scope,
};

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac::require_org_member;
use crate::AppState;

// ─── Query parameter types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ServiceLogQueryParams {
    #[serde(flatten)]
    pub base: LogQueryParams,
    /// When true, includes logs from previous/crashed container replicas
    pub previous: Option<bool>,
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReplicaInfo {
    pub id: Uuid,
    pub docker_container_id: String,
    pub docker_task_id: Option<String>,
    pub node_id: Option<String>,
    pub replica_index: Option<i32>,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org_id/logs", get(get_org_logs))
        .route("/orgs/:org_id/logs/stats", get(get_org_logs_stats))
        .route("/projects/:project_id/logs", get(get_project_logs))
        .route("/services/:service_id/logs", get(get_service_logs))
        .route("/services/:service_id/replicas", get(list_replicas))
        .route("/services/:service_id/logs/stream", get(log_stream))
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /api/orgs/:org_id/logs
async fn get_org_logs(
    auth_user: AuthUser,
    Path(org_id): Path<Uuid>,
    Query(params): Query<LogQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<LogPage>>, ApiAppError> {
    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    let query = ObsLogQuery::new(Scope::Org(org_id), params, Utc::now())
        .map_err(|e| ApiAppError(AppError::BadRequest(e.to_string())))?;

    let store = PgLogStore::new(state.db.clone());
    let page = store
        .query_logs(&query)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(page)))
}

/// GET /api/orgs/:org_id/logs/stats
async fn get_org_logs_stats(
    auth_user: AuthUser,
    Path(org_id): Path<Uuid>,
    Query(params): Query<LogStatsParams>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<LogStatsResult>>, ApiAppError> {
    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    let query = shipyard_observability::LogStatsQuery::new(Some(org_id), params, Utc::now())
        .map_err(|e| ApiAppError(AppError::BadRequest(e.to_string())))?;

    let store = PgLogStore::new(state.db.clone());
    let stats = store
        .query_stats(&query)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(stats)))
}

/// GET /api/projects/:project_id/logs
async fn get_project_logs(
    auth_user: AuthUser,
    Path(project_id): Path<Uuid>,
    Query(mut params): Query<LogQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<LogPage>>, ApiAppError> {
    let org_id: Uuid = sqlx::query_scalar("SELECT org_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::NotFound("Project not found".into())))?;

    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    params.project_id = Some(project_id);
    let query = ObsLogQuery::new(Scope::Org(org_id), params, Utc::now())
        .map_err(|e| ApiAppError(AppError::BadRequest(e.to_string())))?;

    let store = PgLogStore::new(state.db.clone());
    let page = store
        .query_logs(&query)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(page)))
}

/// GET /api/services/:service_id/logs
async fn get_service_logs(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    Query(mut params): Query<ServiceLogQueryParams>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<LogPage>>, ApiAppError> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT p.org_id, s.project_id FROM services s \
         JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (org_id, project_id) = row.ok_or_else(|| ApiAppError(AppError::NotFound("Service not found".into())))?;

    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    params.base.project_id = Some(project_id);
    params.base.service_id = Some(service_id);

    let query = ObsLogQuery::new(Scope::Org(org_id), params.base, Utc::now())
        .map_err(|e| ApiAppError(AppError::BadRequest(e.to_string())))?;

    let store = PgLogStore::new(state.db.clone());
    let page = store
        .query_logs(&query)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(page)))
}

/// GET /services/:service_id/replicas
async fn list_replicas(
    _auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<ReplicaInfo>>>, ApiAppError> {
    verify_service_exists(&state.db, service_id).await?;

    let replicas = sqlx::query_as::<_, ReplicaInfo>(
        "SELECT id, docker_container_id, docker_task_id, node_id, replica_index, status, started_at
         FROM service_replicas
         WHERE service_id = $1
         ORDER BY replica_index ASC NULLS LAST, started_at DESC NULLS LAST",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(replicas)))
}

/// GET /services/:service_id/logs/stream
async fn log_stream(
    _auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiAppError> {
    verify_service_exists(&state.db, service_id).await?;

    let log_stream = stream::unfold((), move |_| async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let event = Event::default().event("log").data(format!(
            "{{\"timestamp\":\"{}\",\"message\":\"Container running...\"}}",
            Utc::now().to_rfc3339()
        ));
        Some((Ok(event), ()))
    });

    Ok(Sse::new(log_stream).keep_alive(KeepAlive::default()))
}

async fn verify_service_exists(db: &sqlx::PgPool, service_id: Uuid) -> Result<(), ApiAppError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM services WHERE id = $1)")
        .bind(service_id)
        .fetch_one(db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if !exists {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Service '{}' not found",
            service_id
        ))));
    }

    Ok(())
}
