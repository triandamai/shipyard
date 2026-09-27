use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::time::Duration;
use uuid::Uuid;

use crate::{error::ApiAppError, middleware::rbac, AppState};
use shipyard_cloudflare::CloudflareClient;
use shipyard_common::error::AppError;
use shipyard_db::models::CloudflareConnection;

/// `reqwest::Client::new()` (used inside `shipyard_cloudflare::CloudflareClient`)
/// has no default timeout, so every Cloudflare HTTP call in this file is
/// wrapped in `tokio::time::timeout` with this bound — mirrors
/// `resources::mod.rs::CLOUDFLARE_CALL_TIMEOUT` and
/// `settings::mod.rs::CLOUDFLARE_CALL_TIMEOUT`. These are foreground,
/// user-facing handlers (not background best-effort paths), so a timeout is
/// surfaced to the caller as a clear error rather than swallowed.
const CLOUDFLARE_CALL_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
pub struct ConnectCloudflareRequest {
    pub api_token: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org_id/cloudflare", get(get_connection).post(connect).delete(disconnect))
}

async fn require_read(state: &AppState, user_id: Uuid, org_id: Uuid) -> Result<(), ApiAppError> {
    let ok = rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:providers:read")).await.is_ok()
        || rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:settings:read")).await.is_ok();
    if ok { Ok(()) } else { Err(ApiAppError(AppError::Forbidden("providers:read or settings:read required".to_string()))) }
}

async fn require_write(state: &AppState, user_id: Uuid, org_id: Uuid) -> Result<(), ApiAppError> {
    let ok = rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:providers:write")).await.is_ok()
        || rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:settings:write")).await.is_ok();
    if ok { Ok(()) } else { Err(ApiAppError(AppError::Forbidden("providers:write or settings:write required".to_string()))) }
}

/// GET /orgs/:org_id/cloudflare — connection status plus a live zone list
/// (fetched fresh from Cloudflare every call; no local zone cache).
#[derive(serde::Serialize)]
struct ConnectionStatus {
    #[serde(flatten)]
    connection: Option<CloudflareConnection>,
    zones: Vec<shipyard_cloudflare::Zone>,
}

async fn get_connection(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<crate::ApiResponse<ConnectionStatus>>, ApiAppError> {
    require_read(&state, auth.user_id, org_id).await?;

    let connection = sqlx::query_as::<_, CloudflareConnection>(
        "SELECT id, org_id, api_token, account_id, account_name, created_at
         FROM cloudflare_connections WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let zones = match &connection {
        Some(c) => {
            let client = CloudflareClient::new(c.api_token.clone());
            match tokio::time::timeout(CLOUDFLARE_CALL_TIMEOUT, client.list_zones()).await {
                Ok(Ok(z)) => z,
                Ok(Err(e)) => return Err(ApiAppError(e)),
                Err(_) => {
                    return Err(ApiAppError(AppError::Cloudflare(
                        "Cloudflare list_zones timed out".to_string(),
                    )))
                }
            }
        }
        None => Vec::new(),
    };

    Ok(Json(crate::ApiResponse::ok(ConnectionStatus { connection, zones })))
}

/// POST /orgs/:org_id/cloudflare — verifies the token, resolves the account
/// for display from the token's first accessible zone (a `GET /accounts`
/// call would require an account-level permission a zone-scoped
/// `Zone:DNS:Edit` token doesn't have — see `Zone::account`), then stores
/// the connection. One per org (UNIQUE constraint); connecting again
/// replaces the existing one.
async fn connect(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<ConnectCloudflareRequest>,
) -> Result<(StatusCode, Json<crate::ApiResponse<CloudflareConnection>>), ApiAppError> {
    require_write(&state, auth.user_id, org_id).await?;

    if body.api_token.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("api_token is required".to_string())));
    }

    let client = CloudflareClient::new(body.api_token.clone());
    match tokio::time::timeout(CLOUDFLARE_CALL_TIMEOUT, client.verify_token()).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return Err(ApiAppError(AppError::BadRequest(format!(
                "Could not verify Cloudflare token: {e}"
            ))));
        }
        Err(_) => {
            return Err(ApiAppError(AppError::Cloudflare(
                "Cloudflare verify_token timed out".to_string(),
            )));
        }
    }

    let zones = match tokio::time::timeout(CLOUDFLARE_CALL_TIMEOUT, client.list_zones()).await {
        Ok(Ok(z)) => z,
        Ok(Err(e)) => return Err(ApiAppError(e)),
        Err(_) => {
            return Err(ApiAppError(AppError::Cloudflare(
                "Cloudflare list_zones timed out".to_string(),
            )))
        }
    };
    let account = zones.into_iter().next().map(|z| z.account).ok_or_else(|| {
        ApiAppError(AppError::BadRequest(
            "This Cloudflare token has no accessible zones — create a token with Zone:DNS:Edit permission for at least one zone".to_string(),
        ))
    })?;

    let connection_id = Uuid::now_v7();
    let connection = sqlx::query_as::<_, CloudflareConnection>(
        "INSERT INTO cloudflare_connections (id, org_id, api_token, account_id, account_name, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW())
         ON CONFLICT (org_id) DO UPDATE
           SET api_token = EXCLUDED.api_token, account_id = EXCLUDED.account_id,
               account_name = EXCLUDED.account_name, created_at = NOW()
         RETURNING id, org_id, api_token, account_id, account_name, created_at",
    )
    .bind(connection_id)
    .bind(org_id)
    .bind(&body.api_token)
    .bind(&account.id)
    .bind(&account.name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok((StatusCode::CREATED, Json(crate::ApiResponse::ok(connection))))
}

/// DELETE /orgs/:org_id/cloudflare — disconnects. Does NOT delete any DNS
/// records this connection created; existing domains' cloudflare_record_id
/// simply becomes unreachable for future deletes (see the spec's
/// "Failure handling" section — a delete_domain call with no connection to
/// authenticate against just skips the Cloudflare step, same as an org that
/// never connected at all).
async fn disconnect(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<crate::ApiResponse<()>>, ApiAppError> {
    require_write(&state, auth.user_id, org_id).await?;

    sqlx::query("DELETE FROM cloudflare_connections WHERE org_id = $1")
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(crate::ApiResponse::ok(())))
}
