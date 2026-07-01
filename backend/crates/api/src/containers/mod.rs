use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::{ApiResponse, MqttPayload, Paginated};
use shipyard_db::models::{Container, DockerEvent};
use shipyard_docker::types::ContainerDetail;
use shipyard_mqtt::topics;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac::{require_project_access, require_service_access};
use crate::AppState;

// ─── Query parameter types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ContainerFilter {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DockerEventQuery {
    pub actor_id: Option<String>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub from: Option<chrono::DateTime<chrono::Utc>>,
    pub to: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/services/:service_id/containers",
            get(list_service_containers),
        )
        .route(
            "/services/:service_id/containers/:container_id",
            get(get_container).delete(delete_container),
        )
        .route(
            "/services/:service_id/containers/:container_id/inspect",
            get(inspect_container),
        )
        .route(
            "/services/:service_id/containers/:container_id/stop",
            post(stop_container),
        )
        .route(
            "/services/:service_id/containers/:container_id/restart",
            post(restart_container),
        )
        .route(
            "/projects/:project_id/containers",
            get(list_project_containers),
        )
        .route("/docker-events", get(list_docker_events))
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /services/:service_id/containers?status=running|failed|all
async fn list_service_containers(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    Query(filter): Query<ContainerFilter>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Container>>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let containers = match filter.status.as_deref() {
        Some("all") | None => {
            sqlx::query_as::<_, Container>(
                "SELECT id, service_id, docker_container_id, docker_task_id, node_id,
                        replica_index, status::text AS status, status_message, image,
                        started_at, finished_at, exit_code, created_at, updated_at
                 FROM containers
                 WHERE service_id = $1
                 ORDER BY created_at DESC",
            )
            .bind(service_id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        }
        Some(status_filter) => {
            sqlx::query_as::<_, Container>(
                "SELECT id, service_id, docker_container_id, docker_task_id, node_id,
                        replica_index, status::text AS status, status_message, image,
                        started_at, finished_at, exit_code, created_at, updated_at
                 FROM containers
                 WHERE service_id = $1 AND status::text = $2
                 ORDER BY created_at DESC",
            )
            .bind(service_id)
            .bind(status_filter)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        }
    };

    Ok(Json(ApiResponse::ok(containers)))
}

/// GET /services/:service_id/containers/:container_id
async fn get_container(
    auth_user: AuthUser,
    Path((service_id, container_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Container>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let container = sqlx::query_as::<_, Container>(
        "SELECT id, service_id, docker_container_id, docker_task_id, node_id,
                replica_index, status::text AS status, status_message, image,
                started_at, finished_at, exit_code, created_at, updated_at
         FROM containers
         WHERE id = $1 AND service_id = $2",
    )
    .bind(container_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!(
            "Container '{}' not found",
            container_id
        )))
    })?;

    Ok(Json(ApiResponse::ok(container)))
}

/// DELETE /services/:service_id/containers/:container_id
///
/// Removes the container DB record. Only permitted for terminal statuses
/// (shutdown, failed, orphan, complete, rejected). Running containers must
/// be stopped via Docker first.
async fn delete_container(
    auth_user: AuthUser,
    Path((service_id, container_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    // Fetch the container and check it belongs to this service and is in a terminal state.
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT status::text FROM containers WHERE id = $1 AND service_id = $2",
    )
    .bind(container_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let status = match row {
        Some((s,)) => s,
        None => return Err(ApiAppError(AppError::NotFound(format!(
            "Container '{}' not found",
            container_id
        )))),
    };

    const TERMINAL: &[&str] = &["shutdown", "failed", "orphan", "complete", "rejected"];
    if !TERMINAL.contains(&status.as_str()) {
        return Err(ApiAppError(AppError::BadRequest(format!(
            "Container is still '{}' — stop it before deleting the record",
            status
        ))));
    }

    sqlx::query("DELETE FROM containers WHERE id = $1")
        .bind(container_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    tracing::info!(%container_id, %service_id, status = %status, "Container record deleted by user");

    if let Ok(Some((project_id, org_id))) = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT p.id, p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({
                "resource": "container",
                "container_id": container_id,
                "reason": "deleted"
            }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok((StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "message": "Container record deleted"
    })))))
}

/// GET /services/:service_id/containers/:container_id/inspect
///
/// Calls Docker inspect on the live container and returns rich runtime detail.
async fn inspect_container(
    auth_user: AuthUser,
    Path((service_id, container_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ContainerDetail>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    // Look up the Docker container ID from our DB record
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT docker_container_id FROM containers WHERE id = $1 AND service_id = $2",
    )
    .bind(container_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Container '{}' not found", container_id))))?;

    let docker_id = row.0;
    if docker_id.is_empty() {
        return Err(ApiAppError(AppError::BadRequest(
            "Container has no Docker ID yet — it may not have started".to_string(),
        )));
    }

    let detail = state
        .docker
        .inspect_container(&docker_id)
        .await
        .map_err(ApiAppError)?;

    Ok(Json(ApiResponse::ok(detail)))
}

/// POST /services/:service_id/containers/:container_id/stop
async fn stop_container(
    auth_user: AuthUser,
    Path((service_id, container_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let row = sqlx::query_as::<_, (String,)>(
        "SELECT docker_container_id FROM containers WHERE id = $1 AND service_id = $2",
    )
    .bind(container_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Container '{}' not found", container_id))))?;

    let docker_id = row.0;
    if docker_id.is_empty() {
        return Err(ApiAppError(AppError::BadRequest(
            "Container has no Docker ID yet".to_string(),
        )));
    }

    state.docker.stop_container(&docker_id, 10).await.map_err(ApiAppError)?;

    if let Ok(Some((project_id, org_id))) = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT p.id, p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({ "resource": "container", "container_id": container_id, "reason": "stopped" }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Container stopped" }))))
}

/// POST /services/:service_id/containers/:container_id/restart
async fn restart_container(
    auth_user: AuthUser,
    Path((service_id, container_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    let row = sqlx::query_as::<_, (String,)>(
        "SELECT docker_container_id FROM containers WHERE id = $1 AND service_id = $2",
    )
    .bind(container_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Container '{}' not found", container_id))))?;

    let docker_id = row.0;
    if docker_id.is_empty() {
        return Err(ApiAppError(AppError::BadRequest(
            "Container has no Docker ID yet".to_string(),
        )));
    }

    state.docker.restart_container(&docker_id, 10).await.map_err(ApiAppError)?;

    if let Ok(Some((project_id, org_id))) = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT p.id, p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    {
        let topic = topics::topology(org_id, project_id);
        let payload = MqttPayload::new("topology.changed")
            .with_meta(serde_json::json!({ "resource": "container", "container_id": container_id, "reason": "restarted" }));
        state.mqtt.publish_status(&topic, &payload).await.ok();
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Container restarted" }))))
}

/// GET /projects/:project_id/containers
async fn list_project_containers(
    auth_user: AuthUser,
    Path(project_id): Path<Uuid>,
    Query(filter): Query<ContainerFilter>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Container>>>, ApiAppError> {
    require_project_access(&state.db, auth_user.user_id, project_id).await.map_err(ApiAppError)?;
    let containers = match filter.status.as_deref() {
        Some("all") | None => {
            sqlx::query_as::<_, Container>(
                "SELECT c.id, c.service_id, c.docker_container_id, c.docker_task_id, c.node_id,
                        c.replica_index, c.status::text AS status, c.status_message, c.image,
                        c.started_at, c.finished_at, c.exit_code, c.created_at, c.updated_at
                 FROM containers c
                 JOIN services s ON s.id = c.service_id
                 WHERE s.project_id = $1
                 ORDER BY c.created_at DESC",
            )
            .bind(project_id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        }
        Some(status_filter) => {
            sqlx::query_as::<_, Container>(
                "SELECT c.id, c.service_id, c.docker_container_id, c.docker_task_id, c.node_id,
                        c.replica_index, c.status::text AS status, c.status_message, c.image,
                        c.started_at, c.finished_at, c.exit_code, c.created_at, c.updated_at
                 FROM containers c
                 JOIN services s ON s.id = c.service_id
                 WHERE s.project_id = $1 AND c.status::text = $2
                 ORDER BY c.created_at DESC",
            )
            .bind(project_id)
            .bind(status_filter)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        }
    };

    Ok(Json(ApiResponse::ok(containers)))
}

/// GET /docker-events?actor_id=&type=&from=&to=&page=&per_page=
///
/// Restricted to org admins and owners — raw Docker events contain actor IDs
/// for every container on the host and must not be exposed to regular members.
async fn list_docker_events(
    auth_user: AuthUser,
    Query(query): Query<DockerEventQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Paginated<DockerEvent>>>, ApiAppError> {
    // Require admin or owner role in at least one organization.
    let is_admin: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE user_id = $1 AND role IN ('admin', 'owner') LIMIT 1",
    )
    .bind(auth_user.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if is_admin.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "Docker events are restricted to org admins".to_string(),
        )));
    }
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 200);
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    // Build dynamic WHERE clauses
    // We compose a single query with optional filter conditions.
    // SQLx doesn't easily support truly dynamic queries, so we enumerate
    // the small set of filter combinations by checking Option fields.

    let (events, total) = match (&query.actor_id, &query.event_type, &query.from, &query.to) {
        (None, None, None, None) => {
            let events = sqlx::query_as::<_, DockerEvent>(
                "SELECT id, event_type, action, actor_id, actor_attributes, scope, raw, received_at
                 FROM docker_events
                 ORDER BY received_at DESC
                 LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            let (total,): (i64,) = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*) FROM docker_events",
            )
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            (events, total)
        }
        _ => {
            // Build the WHERE clause dynamically using a text buffer and re-using
            // the concrete typed query approach that SQLx supports.
            // For simplicity, we use a CTE-style approach with all optional filters:
            let events = sqlx::query_as::<_, DockerEvent>(
                "SELECT id, event_type, action, actor_id, actor_attributes, scope, raw, received_at
                 FROM docker_events
                 WHERE ($1::TEXT IS NULL OR actor_id = $1)
                   AND ($2::TEXT IS NULL OR event_type = $2)
                   AND ($3::TIMESTAMPTZ IS NULL OR received_at >= $3)
                   AND ($4::TIMESTAMPTZ IS NULL OR received_at <= $4)
                 ORDER BY received_at DESC
                 LIMIT $5 OFFSET $6",
            )
            .bind(&query.actor_id)
            .bind(&query.event_type)
            .bind(query.from)
            .bind(query.to)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            let (total,): (i64,) = sqlx::query_as::<_, (i64,)>(
                "SELECT COUNT(*)
                 FROM docker_events
                 WHERE ($1::TEXT IS NULL OR actor_id = $1)
                   AND ($2::TEXT IS NULL OR event_type = $2)
                   AND ($3::TIMESTAMPTZ IS NULL OR received_at >= $3)
                   AND ($4::TIMESTAMPTZ IS NULL OR received_at <= $4)",
            )
            .bind(&query.actor_id)
            .bind(&query.event_type)
            .bind(query.from)
            .bind(query.to)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            (events, total)
        }
    };

    Ok(Json(ApiResponse::ok(Paginated {
        data: events,
        page,
        per_page,
        total,
    })))
}
