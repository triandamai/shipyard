use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    auth::{ApiKeyUser, SCOPE_READ},
    error::OpenApiError,
    response::OkResponse,
    OpenApiState,
};

pub fn routes() -> Router<OpenApiState> {
    Router::new()
        .route("/orgs", get(get_org))
        .route("/orgs/:org_id", get(get_org_by_id))
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct OrgResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /openapi/v1/orgs
/// Returns the organization this key belongs to.
async fn get_org(
    caller: ApiKeyUser,
    State(state): State<OpenApiState>,
) -> Result<Json<OkResponse<OrgResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    let org = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
        "SELECT id, name, slug, created_at FROM organizations WHERE id = $1",
    )
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OpenApiError::NotFound(format!("Organization '{}' not found", caller.org_id)))?;

    let (id, name, slug, created_at) = org;
    Ok(Json(OkResponse::new(OrgResponse { id, name, slug, created_at })))
}

/// GET /openapi/v1/orgs/:org_id
/// Convenience alias — only resolves to the caller's own org.
async fn get_org_by_id(
    caller: ApiKeyUser,
    Path(org_id): Path<Uuid>,
    State(state): State<OpenApiState>,
) -> Result<Json<OkResponse<OrgResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    if org_id != caller.org_id {
        return Err(OpenApiError::Forbidden(
            "This API key can only access its own organization".into(),
        ));
    }

    let org = sqlx::query_as::<_, (Uuid, String, String, DateTime<Utc>)>(
        "SELECT id, name, slug, created_at FROM organizations WHERE id = $1",
    )
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OpenApiError::NotFound(format!("Organization '{}' not found", org_id)))?;

    let (id, name, slug, created_at) = org;
    Ok(Json(OkResponse::new(OrgResponse { id, name, slug, created_at })))
}
