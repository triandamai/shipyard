use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{ApiKeyUser, SCOPE_READ, SCOPE_WRITE},
    error::OpenApiError,
    response::{OkResponse, PageParams, PageResponse},
    OpenApiState,
};

pub fn routes() -> Router<OpenApiState> {
    Router::new()
        .route("/projects/:project_id/services", get(list_services))
        .route("/services/:service_id", get(get_service).patch(update_service))
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ServiceResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub slug: String,
    #[serde(rename = "type")]
    pub service_type: String,
    pub status: String,
    pub replicas: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct UpdateServiceRequest {
    pub name: Option<String>,
    pub replicas: Option<i32>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /openapi/v1/projects/:project_id/services
async fn list_services(
    caller: ApiKeyUser,
    Path(project_id): Path<Uuid>,
    Query(page): Query<PageParams>,
    State(state): State<OpenApiState>,
) -> Result<Json<PageResponse<ServiceResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    // Verify project belongs to caller's org
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM projects WHERE id = $1 AND org_id = $2",
    )
    .bind(project_id)
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await?;

    if exists.is_none() {
        return Err(OpenApiError::NotFound(format!("Project '{}' not found", project_id)));
    }

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM services WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_one(&state.db)
    .await?;

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, String, i32, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, project_id, name, slug, type::text, status, replicas, created_at, updated_at
         FROM services
         WHERE project_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(project_id)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&state.db)
    .await?;

    let services = rows
        .into_iter()
        .map(|(id, project_id, name, slug, service_type, status, replicas, created_at, updated_at)| {
            ServiceResponse { id, project_id, name, slug, service_type, status, replicas, created_at, updated_at }
        })
        .collect();

    Ok(Json(PageResponse::new(services, total.0, page.page, page.per_page)))
}

/// GET /openapi/v1/services/:service_id
async fn get_service(
    caller: ApiKeyUser,
    Path(service_id): Path<Uuid>,
    State(state): State<OpenApiState>,
) -> Result<Json<OkResponse<ServiceResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, String, i32, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT s.id, s.project_id, s.name, s.slug, s.type::text, s.status,
                s.replicas, s.created_at, s.updated_at
         FROM services s
         JOIN projects p ON p.id = s.project_id
         WHERE s.id = $1 AND p.org_id = $2",
    )
    .bind(service_id)
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OpenApiError::NotFound(format!("Service '{}' not found", service_id)))?;

    let (id, project_id, name, slug, service_type, status, replicas, created_at, updated_at) = row;
    Ok(Json(OkResponse::new(ServiceResponse {
        id, project_id, name, slug, service_type, status, replicas, created_at, updated_at,
    })))
}

/// PATCH /openapi/v1/services/:service_id
async fn update_service(
    caller: ApiKeyUser,
    Path(service_id): Path<Uuid>,
    State(state): State<OpenApiState>,
    Json(body): Json<UpdateServiceRequest>,
) -> Result<(StatusCode, Json<OkResponse<ServiceResponse>>), OpenApiError> {
    caller.require_scope(SCOPE_WRITE)?;

    // Ensure the service belongs to the caller's org
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

    if body.name.is_none() && body.replicas.is_none() {
        return Err(OpenApiError::BadRequest(
            "At least one field (name, replicas) must be provided".into(),
        ));
    }

    if let Some(ref name) = body.name {
        if name.trim().is_empty() {
            return Err(OpenApiError::BadRequest("Service name must not be empty".into()));
        }
    }

    if let Some(replicas) = body.replicas {
        if replicas < 0 {
            return Err(OpenApiError::BadRequest("Replicas must be >= 0".into()));
        }
    }

    // Build dynamic update — only touch provided fields
    match (body.name.as_deref(), body.replicas) {
        (Some(name), Some(replicas)) => {
            sqlx::query(
                "UPDATE services SET name = $1, replicas = $2, updated_at = NOW() WHERE id = $3",
            )
            .bind(name)
            .bind(replicas)
            .bind(service_id)
            .execute(&state.db)
            .await?;
        }
        (Some(name), None) => {
            sqlx::query("UPDATE services SET name = $1, updated_at = NOW() WHERE id = $2")
                .bind(name)
                .bind(service_id)
                .execute(&state.db)
                .await?;
        }
        (None, Some(replicas)) => {
            sqlx::query("UPDATE services SET replicas = $1, updated_at = NOW() WHERE id = $2")
                .bind(replicas)
                .bind(service_id)
                .execute(&state.db)
                .await?;
        }
        (None, None) => unreachable!("validated above"),
    }

    // Re-fetch and return updated record
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, String, String, i32, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT s.id, s.project_id, s.name, s.slug, s.type::text, s.status,
                s.replicas, s.created_at, s.updated_at
         FROM services s
         WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_one(&state.db)
    .await?;

    let (id, project_id, name, slug, service_type, status, replicas, created_at, updated_at) = row;
    Ok((
        StatusCode::OK,
        Json(OkResponse::new(ServiceResponse {
            id, project_id, name, slug, service_type, status, replicas, created_at, updated_at,
        })),
    ))
}
