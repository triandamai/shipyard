use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{generate_key, ApiKeyUser, SCOPE_ADMIN},
    error::OpenApiError,
    response::{OkResponse, PageParams, PageResponse},
    OpenApiState,
};

pub fn routes() -> Router<OpenApiState> {
    Router::new()
        .route("/keys", get(list_keys).post(create_key))
        .route("/keys/:key_id", delete(revoke_key))
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ApiKeyInfo {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct CreatedKey {
    pub id: Uuid,
    pub name: String,
    pub key: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateKeyRequest {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /openapi/v1/keys
async fn list_keys(
    caller: ApiKeyUser,
    Query(page): Query<PageParams>,
    State(state): State<OpenApiState>,
) -> Result<Json<PageResponse<ApiKeyInfo>>, OpenApiError> {
    caller.require_scope(SCOPE_ADMIN)?;

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM api_keys WHERE org_id = $1",
    )
    .bind(caller.org_id)
    .fetch_one(&state.db)
    .await?;

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, Vec<String>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, DateTime<Utc>)>(
        "SELECT id, org_id, name, key_prefix, scopes, last_used_at, expires_at, created_at
         FROM api_keys
         WHERE org_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(caller.org_id)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&state.db)
    .await?;

    let keys = rows
        .into_iter()
        .map(|(id, org_id, name, key_prefix, scopes, last_used_at, expires_at, created_at)| {
            ApiKeyInfo { id, org_id, name, key_prefix, scopes, last_used_at, expires_at, created_at }
        })
        .collect();

    Ok(Json(PageResponse::new(keys, total.0, page.page, page.per_page)))
}

/// POST /openapi/v1/keys
async fn create_key(
    caller: ApiKeyUser,
    State(state): State<OpenApiState>,
    Json(body): Json<CreateKeyRequest>,
) -> Result<(StatusCode, Json<OkResponse<CreatedKey>>), OpenApiError> {
    caller.require_scope(SCOPE_ADMIN)?;

    if body.name.trim().is_empty() {
        return Err(OpenApiError::BadRequest("Key name must not be empty".into()));
    }

    let valid_scopes = ["read", "deploy", "write", "admin"];
    for s in &body.scopes {
        if !valid_scopes.contains(&s.as_str()) {
            return Err(OpenApiError::BadRequest(format!(
                "Unknown scope '{}'. Valid scopes: {}",
                s,
                valid_scopes.join(", ")
            )));
        }
    }

    let (full_key, prefix, hash) = generate_key();
    let key_id = Uuid::new_v4();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO api_keys (id, org_id, created_by, name, key_prefix, key_hash, scopes, expires_at, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(key_id)
    .bind(caller.org_id)
    .bind(caller.key_id) // the key that created this key is the creator
    .bind(&body.name)
    .bind(&prefix)
    .bind(&hash)
    .bind(&body.scopes)
    .bind(body.expires_at)
    .bind(now)
    .execute(&state.db)
    .await?;

    tracing::info!(key_id = %key_id, org_id = %caller.org_id, "API key created: {}", body.name);

    Ok((
        StatusCode::CREATED,
        Json(OkResponse::new(CreatedKey {
            id: key_id,
            name: body.name,
            key: full_key,
            key_prefix: prefix,
            scopes: body.scopes,
            expires_at: body.expires_at,
            created_at: now,
        })),
    ))
}

/// DELETE /openapi/v1/keys/:key_id
async fn revoke_key(
    caller: ApiKeyUser,
    Path(key_id): Path<Uuid>,
    State(state): State<OpenApiState>,
) -> Result<StatusCode, OpenApiError> {
    caller.require_scope(SCOPE_ADMIN)?;

    // Prevent revoking your own key mid-request
    if key_id == caller.key_id {
        return Err(OpenApiError::BadRequest(
            "Cannot revoke the key currently authenticating this request".into(),
        ));
    }

    let result = sqlx::query(
        "DELETE FROM api_keys WHERE id = $1 AND org_id = $2",
    )
    .bind(key_id)
    .bind(caller.org_id)
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(OpenApiError::NotFound(format!("API key '{}' not found", key_id)));
    }

    tracing::info!(key_id = %key_id, org_id = %caller.org_id, "API key revoked");

    Ok(StatusCode::NO_CONTENT)
}
