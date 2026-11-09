//! Superadmin management of integrations and their keys, under
//! `/api/admin/integrations` (dashboard session auth).

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::crypto;
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
        .route("/:integration_id", delete(delete_integration))
        .route("/:integration_id/keys", post(create_key))
        .route("/:integration_id/keys/:key_id", delete(revoke_key))
        .route(
            "/:integration_id/webhooks",
            get(list_admin_webhooks).post(create_admin_webhook),
        )
        .route(
            "/:integration_id/webhooks/:webhook_id",
            delete(delete_admin_webhook),
        )
        .route(
            "/:integration_id/webhooks/:webhook_id/test",
            post(test_admin_webhook),
        )
        .route("/:integration_id/orgs", get(list_admin_integration_orgs))
        .route(
            "/:integration_id/orgs/:org_id",
            put(claim_org).delete(release_org),
        )
}

pub fn generate_webhook_secret() -> String {
    format!("whsec_{}", random_hex(24))
}

pub fn validate_integration_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err(AppError::BadRequest("name must be 1-64 characters".into()));
    }
    Ok(trimmed.to_string())
}

pub fn validate_key_scopes(scopes: &[String]) -> Result<(), AppError> {
    if scopes.is_empty()
        || scopes
            .iter()
            .any(|s| s != SCOPE_MANAGE && s != SCOPE_REVEAL)
    {
        return Err(AppError::BadRequest(format!(
            "scopes must be a non-empty subset of ['{SCOPE_MANAGE}', '{SCOPE_REVEAL}']"
        )));
    }
    Ok(())
}

pub fn generate_integration_key() -> (String, String, String) {
    let key = format!("{KEY_PREFIX}{}", random_hex(32));
    let prefix = key[..KEY_PREFIX.len() + 8].to_string();
    let hash = hash_key(&key);
    (key, prefix, hash)
}

#[derive(Serialize)]
pub struct IntegrationOut {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub org_count: i64,
    pub keys: Vec<KeyOut>,
    pub orgs: Vec<LinkedOrgOut>,
}

#[derive(Serialize)]
pub struct LinkedOrgOut {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub external_id: Option<String>,
}

#[derive(Serialize)]
pub struct KeyOut {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
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

        let orgs = sqlx::query_as::<_, (Uuid, String, String, Option<String>)>(
            "SELECT id, name, slug, external_id FROM organizations WHERE integration_id = $1 ORDER BY name",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
        .into_iter()
        .map(|(id, name, slug, external_id)| LinkedOrgOut {
            id,
            name,
            slug,
            external_id,
        })
        .collect();

        out.push(IntegrationOut {
            id,
            name,
            created_at,
            org_count,
            keys,
            orgs,
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
    let name = validate_integration_name(&body.name).map_err(ApiAppError)?;
    let id: Uuid =
        sqlx::query_scalar("INSERT INTO integrations (id, name) VALUES ($1, $2) RETURNING id")
            .bind(Uuid::now_v7())
            .bind(&name)
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

async fn delete_integration(
    auth: AuthUser,
    Path(integration_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    let n = sqlx::query("DELETE FROM integrations WHERE id = $1")
        .bind(integration_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?
        .rows_affected();

    if n == 0 {
        return Err(ApiAppError(AppError::NotFound("Integration not found".into())));
    }

    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "delete_integration",
        Some("integration"),
        Some(integration_id),
        None,
        None,
    )
    .await;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": integration_id }))))
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
    validate_key_scopes(&body.scopes).map_err(ApiAppError)?;

    let (key, prefix, hash) = generate_integration_key();
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO integration_keys (id, integration_id, name, key_prefix, key_hash, scopes)
         SELECT $1, i.id, $3, $4, $5, $6 FROM integrations i WHERE i.id = $2
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(integration_id)
    .bind(body.name.trim())
    .bind(&prefix)
    .bind(&hash)
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

#[derive(Deserialize)]
struct ClaimOrg {
    /// The id the integrator uses for this org. Defaults to the org's slug.
    external_id: Option<String>,
}

/// PUT /api/admin/integrations/:integration/orgs/:org — link an existing
/// organization (one created in the dashboard, say) to an integration, so its
/// keys can manage it. Integration keys never reach unlinked orgs.
async fn claim_org(
    auth: AuthUser,
    Path((integration_id, org_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<ClaimOrg>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    if let Some(ext) = &body.external_id {
        super::validate_external_id(ext)?;
    }
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM integrations WHERE id = $1)")
            .bind(integration_id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;
    if !exists {
        return Err(ApiAppError(AppError::NotFound("Integration not found".into())));
    }
    let row: Option<String> = sqlx::query_scalar(
        "UPDATE organizations
         SET integration_id = $1, external_id = COALESCE($3, external_id, slug)
         WHERE id = $2 AND (integration_id IS NULL OR integration_id = $1)
         RETURNING external_id",
    )
    .bind(integration_id)
    .bind(org_id)
    .bind(&body.external_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("duplicate") {
            ApiAppError(AppError::Conflict(
                "This integration already has an organization with that external id".into(),
            ))
        } else {
            db_err(e)
        }
    })?;
    let Some(external_id) = row else {
        return Err(ApiAppError(AppError::Conflict(
            "Organization not found, or it already belongs to another integration".into(),
        )));
    };
    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "claim_org_for_integration",
        Some("organization"),
        Some(org_id),
        None,
        Some(serde_json::json!({ "integration_id": integration_id, "external_id": external_id })),
    )
    .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "org_id": org_id, "external_id": external_id }),
    )))
}

/// DELETE /api/admin/integrations/:integration/orgs/:org — unlink an org. It
/// and its data stay; the integration's keys can no longer reach it.
async fn release_org(
    auth: AuthUser,
    Path((integration_id, org_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    let n = sqlx::query(
        "UPDATE organizations SET integration_id = NULL, external_id = NULL
         WHERE id = $2 AND integration_id = $1",
    )
    .bind(integration_id)
    .bind(org_id)
    .execute(&state.db)
    .await
    .map_err(db_err)?
    .rows_affected();
    if n == 0 {
        return Err(ApiAppError(AppError::NotFound(
            "That organization is not linked to this integration".into(),
        )));
    }
    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "release_org_from_integration",
        Some("organization"),
        Some(org_id),
        None,
        Some(serde_json::json!({ "integration_id": integration_id })),
    )
    .await;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "released": org_id }))))
}

#[derive(Serialize)]
pub struct AdminWebhookOut {
    pub id: Uuid,
    pub url: String,
    pub events: Vec<String>,
    pub enabled: bool,
    pub org: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct CreateAdminWebhook {
    pub url: String,
    #[serde(default = "default_admin_events")]
    pub events: Vec<String>,
    pub org: Option<String>,
}

fn default_admin_events() -> Vec<String> {
    vec!["*".to_string()]
}

async fn list_admin_webhooks(
    auth: AuthUser,
    Path(integration_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<AdminWebhookOut>>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    let rows = sqlx::query_as::<_, (Uuid, String, Vec<String>, bool, Option<String>, DateTime<Utc>)>(
        "SELECT we.id, we.url, we.events, we.enabled, o.external_id, we.created_at
         FROM webhook_endpoints we LEFT JOIN organizations o ON o.id = we.org_id
         WHERE we.integration_id = $1 ORDER BY we.created_at DESC",
    )
    .bind(integration_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    Ok(Json(ApiResponse::ok(
        rows.into_iter()
            .map(|(id, url, events, enabled, org, created_at)| AdminWebhookOut {
                id,
                url,
                events,
                enabled,
                org,
                created_at,
            })
            .collect(),
    )))
}

async fn create_admin_webhook(
    auth: AuthUser,
    Path(integration_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<CreateAdminWebhook>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM integrations WHERE id = $1)")
            .bind(integration_id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;
    if !exists {
        return Err(ApiAppError(AppError::NotFound("Integration not found".into())));
    }

    super::webhooks::validate_url(&body.url)?;
    super::webhooks::validate_patterns(&body.events)?;

    let org_id: Option<Uuid> = match &body.org {
        Some(ext) => {
            let id: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM organizations WHERE (external_id = $1 OR slug = $1) AND integration_id = $2",
            )
            .bind(ext)
            .bind(integration_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err)?;
            Some(id.ok_or_else(|| {
                ApiAppError(AppError::BadRequest(
                    "Organization not found or not linked to this integration".into(),
                ))
            })?)
        }
        None => None,
    };

    let secret = generate_webhook_secret();
    let enc = crypto::encrypt(&state.config.auth.secret_key, &secret).map_err(ApiAppError)?;
    let id = Uuid::now_v7();

    sqlx::query(
        "INSERT INTO webhook_endpoints (id, integration_id, org_id, url, secret_encrypted, events)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(integration_id)
    .bind(org_id)
    .bind(&body.url)
    .bind(&enc)
    .bind(&body.events)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "create_admin_webhook",
        Some("webhook_endpoint"),
        Some(id),
        None,
        Some(serde_json::json!({ "integration_id": integration_id, "url": body.url })),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(serde_json::json!({
            "id": id,
            "url": body.url,
            "events": body.events,
            "org": body.org,
            "secret": secret,
        }))),
    ))
}

async fn delete_admin_webhook(
    auth: AuthUser,
    Path((integration_id, webhook_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    let n = sqlx::query("DELETE FROM webhook_endpoints WHERE id = $1 AND integration_id = $2")
        .bind(webhook_id)
        .bind(integration_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?
        .rows_affected();

    if n == 0 {
        return Err(ApiAppError(AppError::NotFound("Webhook endpoint not found".into())));
    }

    crate::middleware::audit::write_audit_log(
        &state.db,
        &auth,
        "delete_admin_webhook",
        Some("webhook_endpoint"),
        Some(webhook_id),
        None,
        Some(serde_json::json!({ "integration_id": integration_id })),
    )
    .await;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": webhook_id }))))
}

async fn test_admin_webhook(
    auth: AuthUser,
    Path((integration_id, webhook_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT url, secret_encrypted FROM webhook_endpoints WHERE id = $1 AND integration_id = $2",
    )
    .bind(webhook_id)
    .bind(integration_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;

    let (url, secret_enc) = row.ok_or_else(|| {
        ApiAppError(AppError::NotFound("Webhook endpoint not found".into()))
    })?;

    let secret = crypto::decrypt_or_passthrough(&state.config.auth.secret_key, &secret_enc);
    let body = serde_json::json!({
        "id": Uuid::now_v7(),
        "type": "ping",
        "created_at": Utc::now(),
        "org": null,
        "app": null,
        "environment": null,
        "project_id": null,
        "service_id": null,
        "service": null,
        "data": { "webhook_id": webhook_id, "source": "admin_test" },
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Shipyard-Webhooks/1")
        .build()
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;

    let outcome = super::webhooks::send(&client, &url, &secret, Uuid::now_v7(), &body).await;

    Ok(Json(ApiResponse::ok(match outcome {
        Ok(code) => serde_json::json!({
            "ok": true,
            "status_code": code,
            "message": format!("Webhook responded with HTTP {code}")
        }),
        Err((code, err)) => serde_json::json!({
            "ok": false,
            "status_code": code,
            "error": err
        }),
    })))
}

#[derive(Deserialize)]
struct AdminOrgsQuery {
    page: Option<i64>,
    limit: Option<i64>,
    search: Option<String>,
}

#[derive(Serialize)]
pub struct AdminLinkedOrgItem {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub external_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct PaginatedAdminOrgs {
    pub items: Vec<AdminLinkedOrgItem>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
    pub total_pages: i64,
}

async fn list_admin_integration_orgs(
    auth: AuthUser,
    Path(integration_id): Path<Uuid>,
    Query(query): Query<AdminOrgsQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PaginatedAdminOrgs>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let search_pattern = query
        .search
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{s}%"));

    let (total, rows) = if let Some(pattern) = search_pattern {
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organizations
             WHERE integration_id = $1 AND (name ILIKE $2 OR slug ILIKE $2 OR external_id ILIKE $2)",
        )
        .bind(integration_id)
        .bind(&pattern)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

        let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, DateTime<Utc>)>(
            "SELECT id, name, slug, external_id, created_at FROM organizations
             WHERE integration_id = $1 AND (name ILIKE $2 OR slug ILIKE $2 OR external_id ILIKE $2)
             ORDER BY created_at DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(integration_id)
        .bind(&pattern)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;

        (total, rows)
    } else {
        let total: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM organizations WHERE integration_id = $1",
        )
        .bind(integration_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

        let rows = sqlx::query_as::<_, (Uuid, String, String, Option<String>, DateTime<Utc>)>(
            "SELECT id, name, slug, external_id, created_at FROM organizations
             WHERE integration_id = $1
             ORDER BY created_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(integration_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;

        (total, rows)
    };

    let total_pages = (total + limit - 1) / limit.max(1);

    Ok(Json(ApiResponse::ok(PaginatedAdminOrgs {
        items: rows
            .into_iter()
            .map(|(id, name, slug, external_id, created_at)| AdminLinkedOrgItem {
                id,
                name,
                slug,
                external_id,
                created_at,
            })
            .collect(),
        total,
        page,
        limit,
        total_pages,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_integration_name() {
        assert_eq!(validate_integration_name("nomi").unwrap(), "nomi");
        assert_eq!(validate_integration_name("  nomi-prod  ").unwrap(), "nomi-prod");
        assert!(validate_integration_name("").is_err());
        assert!(validate_integration_name("   ").is_err());
        assert!(validate_integration_name(&"a".repeat(65)).is_err());
        assert_eq!(validate_integration_name(&"a".repeat(64)).unwrap().len(), 64);
    }

    #[test]
    fn test_validate_key_scopes() {
        assert!(validate_key_scopes(&[SCOPE_MANAGE.to_string()]).is_ok());
        assert!(validate_key_scopes(&[SCOPE_MANAGE.to_string(), SCOPE_REVEAL.to_string()]).is_ok());
        assert!(validate_key_scopes(&[SCOPE_REVEAL.to_string()]).is_ok());

        // Empty scopes should fail
        assert!(validate_key_scopes(&[]).is_err());
        // Unauthorized / unknown scope should fail
        assert!(validate_key_scopes(&["admin".to_string()]).is_err());
        assert!(validate_key_scopes(&[SCOPE_MANAGE.to_string(), "read".to_string()]).is_err());
    }

    #[test]
    fn test_generate_integration_key_format() {
        let (key, prefix, hash) = generate_integration_key();
        assert!(key.starts_with(KEY_PREFIX), "Key must start with {}", KEY_PREFIX);
        assert_eq!(key.len(), KEY_PREFIX.len() + 64);
        assert!(prefix.starts_with(KEY_PREFIX));
        assert_eq!(prefix.len(), KEY_PREFIX.len() + 8);
        assert_eq!(hash, hash_key(&key));
    }

    #[test]
    fn test_generate_webhook_secret_format() {
        let secret = generate_webhook_secret();
        assert!(secret.starts_with("whsec_"), "Webhook secret must start with whsec_");
        assert_eq!(secret.len(), 6 + 48); // "whsec_" (6) + 24 bytes hex (48)
    }

    #[test]
    fn test_validate_webhook_url() {
        assert!(super::super::webhooks::validate_url("https://nomi.example.com/hooks").is_ok());
        assert!(super::super::webhooks::validate_url("http://localhost:3000/hooks").is_ok());
        assert!(super::super::webhooks::validate_url("ftp://example.com").is_err());
        assert!(super::super::webhooks::validate_url("invalid-url").is_err());
    }

    #[test]
    fn test_validate_webhook_patterns() {
        assert!(super::super::webhooks::validate_patterns(&["*".to_string()]).is_ok());
        assert!(super::super::webhooks::validate_patterns(&["publish.*".to_string()]).is_ok());
        assert!(super::super::webhooks::validate_patterns(&["deployment.succeeded".to_string()]).is_ok());
        assert!(super::super::webhooks::validate_patterns(&[]).is_err());
        assert!(super::super::webhooks::validate_patterns(&["unknown.event.type".to_string()]).is_err());
    }
}
