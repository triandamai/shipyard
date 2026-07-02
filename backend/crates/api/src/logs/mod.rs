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
use shipyard_db::models::DeploymentLog;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Query parameter types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    #[allow(dead_code)]
    pub replica: Option<Uuid>,
    pub level: Option<String>,
    pub from: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
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
        .route("/services/:service_id/logs", get(get_logs))
        .route("/services/:service_id/replicas", get(list_replicas))
        .route("/services/:service_id/logs/stream", get(log_stream))
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /services/:service_id/logs
async fn get_logs(
    _auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    Query(params): Query<LogQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<DeploymentLog>>>, ApiAppError> {
    // Verify service exists
    verify_service_exists(&state.db, service_id).await?;

    let logs = if let Some(level) = &params.level {
        if let Some(from) = params.from {
            // Both level and from filters
            sqlx::query_as::<_, DeploymentLog>(
                "SELECT dl.id, dl.deployment_id, dl.step_id, dl.level::text AS level, dl.message, dl.timestamp
                 FROM deployment_logs dl
                 JOIN deployments d ON d.id = dl.deployment_id
                 WHERE d.service_id = $1
                   AND dl.level::text = $2
                   AND dl.timestamp >= $3
                 ORDER BY dl.timestamp DESC
                 LIMIT $4",
            )
            .bind(service_id)
            .bind(level)
            .bind(from)
            .bind(params.limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        } else {
            // Level filter only
            sqlx::query_as::<_, DeploymentLog>(
                "SELECT dl.id, dl.deployment_id, dl.step_id, dl.level::text AS level, dl.message, dl.timestamp
                 FROM deployment_logs dl
                 JOIN deployments d ON d.id = dl.deployment_id
                 WHERE d.service_id = $1
                   AND dl.level::text = $2
                 ORDER BY dl.timestamp DESC
                 LIMIT $3",
            )
            .bind(service_id)
            .bind(level)
            .bind(params.limit)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        }
    } else if let Some(from) = params.from {
        // From filter only
        sqlx::query_as::<_, DeploymentLog>(
            "SELECT dl.id, dl.deployment_id, dl.step_id, dl.level::text AS level, dl.message, dl.timestamp
             FROM deployment_logs dl
             JOIN deployments d ON d.id = dl.deployment_id
             WHERE d.service_id = $1
               AND dl.timestamp >= $2
             ORDER BY dl.timestamp DESC
             LIMIT $3",
        )
        .bind(service_id)
        .bind(from)
        .bind(params.limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    } else {
        // No filters — base query
        sqlx::query_as::<_, DeploymentLog>(
            "SELECT dl.id, dl.deployment_id, dl.step_id, dl.level::text AS level, dl.message, dl.timestamp
             FROM deployment_logs dl
             JOIN deployments d ON d.id = dl.deployment_id
             WHERE d.service_id = $1
             ORDER BY dl.timestamp DESC
             LIMIT $2",
        )
        .bind(service_id)
        .bind(params.limit)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    };

    Ok(Json(ApiResponse::ok(logs)))
}

/// GET /services/:service_id/replicas
async fn list_replicas(
    _auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<ReplicaInfo>>>, ApiAppError> {
    verify_service_exists(&state.db, service_id).await?;

    let replicas = sqlx::query_as::<_, ReplicaInfo>(
        "SELECT id, docker_container_id, docker_task_id, node_id, replica_index,
                status::text AS status, started_at
         FROM containers
         WHERE service_id = $1
           AND status = 'running'::container_status
         ORDER BY replica_index",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(replicas)))
}

/// GET /services/:service_id/logs/stream — SSE endpoint (polling every 2s)
#[allow(unused)]
async fn log_stream(
    _auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let db = state.db.clone();
    let initial_ts = Utc::now();

    let stream = stream::unfold(
        (db, service_id, initial_ts),
        |(db, service_id, last_ts)| async move {
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Fetch logs newer than last_ts
            let result = sqlx::query_as::<_, DeploymentLog>(
                "SELECT dl.id, dl.deployment_id, dl.step_id, dl.level::text AS level, dl.message, dl.timestamp
                 FROM deployment_logs dl
                 JOIN deployments d ON d.id = dl.deployment_id
                 WHERE d.service_id = $1
                   AND dl.timestamp > $2
                 ORDER BY dl.timestamp ASC
                 LIMIT 50",
            )
            .bind(service_id)
            .bind(last_ts)
            .fetch_all(&db)
            .await;

            let (event, new_ts) = match result {
                Ok(logs) => {
                    let new_ts = logs
                        .last()
                        .map(|l| l.timestamp)
                        .unwrap_or(last_ts);

                    let data = serde_json::to_string(&logs).unwrap_or_else(|_| "[]".to_string());
                    let event = Event::default().data(data);
                    (event, new_ts)
                }
                Err(e) => {
                    tracing::warn!("log_stream: DB error for service {}: {}", service_id, e);
                    let event = Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"{}\"}}", e));
                    (event, last_ts)
                }
            };

            Some((Ok(event), (db, service_id, new_ts)))
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

async fn verify_service_exists(db: &sqlx::PgPool, service_id: Uuid) -> Result<(), ApiAppError> {
    let exists: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Service '{}' not found",
            service_id
        ))));
    }
    Ok(())
}
