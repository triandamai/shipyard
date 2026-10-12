//! Superadmin management of integrations and their keys, under
//! `/api/admin/integrations` (dashboard session auth).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use super::{db_err, hash_key, random_hex, KEY_PREFIX, SCOPE_MANAGE, SCOPE_REVEAL};
use crate::admin::require_superadmin;
use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_integrations).post(create_integration))
        .route("/:integration_id/keys", post(create_key))
        .route("/:integration_id/keys/:key_id", delete(revoke_key))
}

#[derive(Serialize)]
struct IntegrationOut {
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    org_count: i64,
    keys: Vec<KeyOut>,
}

#[derive(Serialize)]
struct KeyOut {
    id: Uuid,
    name: String,
    prefix: String,
    scopes: Vec<String>,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

async fn list_integrations(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<IntegrationOut>>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    let rows = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>, i64)>(
        "SELECT i.id, i.name, i.created_at,
                (SELECT COUNT(*) FROM organizations o WHERE o.integration_id = i.id)
         FROM integrations i ORDER BY i.created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let mut out = Vec::with_capacity(rows.len());
    for (id, name, created_at, org_count) in rows {
        let keys = sqlx::query_as::<
            _,
            (
                Uuid,
                String,
                String,
                Vec<String>,
                DateTime<Utc>,
                Option<DateTime<Utc>>,
                Option<DateTime<Utc>>,
            ),
        >(
            "SELECT id, name, key_prefix, scopes, created_at, last_used_at, revoked_at
             FROM integration_keys WHERE integration_id = $1 ORDER BY created_at",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(
            |(id, name, prefix, scopes, created_at, last_used_at, revoked_at)| KeyOut {
                id,
                name,
                prefix,
                scopes,
                created_at,
                last_used_at,
                revoked_at,
            },
        )
        .collect();
        out.push(IntegrationOut {
            id,
            name,
            created_at,
            org_count,
            keys,
        });
    }
    Ok(Json(ApiResponse::ok(out)))
}

#[derive(Deserialize)]
struct CreateIntegration {
    name: String,
}

async fn create_integration(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateIntegration>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(ApiAppError(AppError::BadRequest(
            "name must be 1-64 characters".into(),
        )));
    }
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO integrations (id, name) VALUES ($1, $2) RETURNING id")
            .bind(Uuid::now_v7())
            .bind(name)
            .fetch_one(&state.db)
            .await
            .map_err(|e| {
                if e.to_string().contains("duplicate") {
                    ApiAppError(AppError::Conflict(format!(
                        "An integration named '{name}' exists"
                    )))
                } else {
                    db_err(e)
                }
            })?;
    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "create_integration",
        Some("integration"),
        Some(id),
        None,
        Some(serde_json::json!({ "name": name })),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(
            serde_json::json!({ "id": id, "name": name }),
        )),
    ))
}

#[derive(Deserialize)]
struct CreateKey {
    name: String,
    #[serde(default = "default_scopes")]
    scopes: Vec<String>,
}

fn default_scopes() -> Vec<String> {
    vec![SCOPE_MANAGE.to_string()]
}

/// Mint a key. The full key is in this response only; it is stored hashed.
async fn create_key(
    auth: AuthUser,
    Path(integration_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<CreateKey>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    if body.scopes.is_empty()
        || body
            .scopes
            .iter()
            .any(|s| s != SCOPE_MANAGE && s != SCOPE_REVEAL)
    {
        return Err(ApiAppError(AppError::BadRequest(format!(
            "scopes must be a non-empty subset of ['{SCOPE_MANAGE}', '{SCOPE_REVEAL}']"
        ))));
    }

    let key = format!("{KEY_PREFIX}{}", random_hex(32));
    let prefix = key[..KEY_PREFIX.len() + 8].to_string();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_keys (id, integration_id, name, key_prefix, key_hash, scopes)
         SELECT $1, i.id, $3, $4, $5, $6 FROM integrations i WHERE i.id = $2
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(integration_id)
    .bind(body.name.trim())
    .bind(&prefix)
    .bind(hash_key(&key))
    .bind(&body.scopes)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Integration not found".into())))?;

    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "create_integration_key",
        Some("integration_key"),
        Some(id),
        None,
        Some(serde_json::json!({ "integration_id": integration_id, "scopes": body.scopes })),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(serde_json::json!({
            "id": id,
            "key": key,
            "prefix": prefix,
            "scopes": body.scopes,
            "note": "Store this key now. It is not shown again.",
        }))),
    ))
}

async fn revoke_key(
    auth: AuthUser,
    Path((integration_id, key_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    let n = sqlx::query(
        "UPDATE integration_keys SET revoked_at = NOW()
         WHERE id = $1 AND integration_id = $2 AND revoked_at IS NULL",
    )
    .bind(key_id)
    .bind(integration_id)
    .execute(&state.db)
    .await
    .map_err(db_err)?
    .rows_affected();
    if n == 0 {
        return Err(ApiAppError(AppError::NotFound(
            "Key not found or already revoked".into(),
        )));
    }
    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "revoke_integration_key",
        Some("integration_key"),
        Some(key_id),
        None,
        None,
    )
    .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "revoked": key_id }),
    )))
}
