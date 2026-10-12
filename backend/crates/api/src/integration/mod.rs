//! Platform integration API (first integrator: Nomi).
//!
//! Mounted at `/openapi/v1/integration`. Authenticated with a platform-level
//! `ship_int_` key (see `integration_keys`), unlike the per-org `ship_` keys of
//! the rest of the Open API. An integration manages the orgs it created:
//!
//! - orgs, addressed by the integrator's own id (`external_id`)
//! - apps (one per Nomi project) with one Shipyard project per environment
//! - the secret store, the shared dev database, publish, duplicate, terminate
//! - outbound webhooks for every event in its orgs
//!
//! Scopes: `manage` for everything except reading secret values, which needs
//! `secrets:reveal`. Give agent tooling a `manage`-only key so a secret value
//! can never end up in a model's context.

pub mod apps;
pub mod devdb;
pub mod keys;
pub mod orgs;
pub mod publish;
pub mod secrets;
pub mod webhooks;

use axum::{async_trait, extract::FromRequestParts, http::request::Parts, Router};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use shipyard_common::error::AppError;

use crate::error::ApiAppError;
use crate::AppState;

pub const SCOPE_MANAGE: &str = "manage";
pub const SCOPE_REVEAL: &str = "secrets:reveal";
pub const KEY_PREFIX: &str = "ship_int_";

pub fn routes() -> Router<AppState> {
    Router::new()
        .merge(orgs::routes())
        .merge(apps::routes())
        .merge(publish::routes())
        .merge(devdb::routes())
        .merge(secrets::routes())
        .merge(webhooks::routes())
}

// ─── Caller ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IntegrationCaller {
    pub integration_id: Uuid,
    pub key_id: Uuid,
    pub key_name: String,
    pub scopes: Vec<String>,
}

impl IntegrationCaller {
    pub fn require(&self, scope: &str) -> Result<(), ApiAppError> {
        if self.scopes.iter().any(|s| s == scope) {
            Ok(())
        } else {
            Err(ApiAppError(AppError::Forbidden(format!(
                "This integration key does not have the '{scope}' scope"
            ))))
        }
    }

    /// Audit-log an action under this key.
    pub async fn audit(
        &self,
        db: &sqlx::PgPool,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) {
        let _ = sqlx::query(
            r#"INSERT INTO audit_logs
                   (id, user_id, actor_type, actor_id, actor_name,
                    action, resource_type, resource_id, ip_address, metadata, created_at)
               VALUES ($1, NULL, 'integration', $2, $3, $4, $5, $6, NULL, $7, NOW())"#,
        )
        .bind(Uuid::now_v7())
        .bind(self.key_id)
        .bind(&self.key_name)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(metadata)
        .execute(db)
        .await;
    }
}

#[async_trait]
impl FromRequestParts<AppState> for IntegrationCaller {
    type Rejection = ApiAppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(str::trim)
            .ok_or_else(|| {
                ApiAppError(AppError::Unauthorized(
                    "Missing integration key. Send 'Authorization: Bearer ship_int_...'.".into(),
                ))
            })?;
        if !raw.starts_with(KEY_PREFIX) {
            return Err(ApiAppError(AppError::Unauthorized(
                "Integration keys start with 'ship_int_'.".into(),
            )));
        }

        let row = sqlx::query_as::<_, (Uuid, Uuid, String, Vec<String>)>(
            "SELECT k.id, k.integration_id, k.name, k.scopes
             FROM integration_keys k
             WHERE k.key_hash = $1 AND k.revoked_at IS NULL",
        )
        .bind(hash_key(raw))
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| {
            ApiAppError(AppError::Unauthorized(
                "Invalid or revoked integration key.".into(),
            ))
        })?;

        let (key_id, integration_id, key_name, scopes) = row;
        let db = state.db.clone();
        tokio::spawn(async move {
            let _ = sqlx::query("UPDATE integration_keys SET last_used_at = NOW() WHERE id = $1")
                .bind(key_id)
                .execute(&db)
                .await;
        });

        Ok(IntegrationCaller {
            integration_id,
            key_id,
            key_name,
            scopes,
        })
    }
}

pub fn hash_key(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

/// `bytes` random bytes from the OS, hex-encoded.
pub fn random_hex(bytes: usize) -> String {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    let mut buf = vec![0u8; bytes];
    OsRng.fill_bytes(&mut buf);
    hex::encode(buf)
}

/// Random lowercase alphanumeric string, safe in hostnames, SQL identifiers
/// and connection strings without quoting.
pub fn random_alnum(len: usize) -> String {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let mut b = [0u8; 1];
        OsRng.fill_bytes(&mut b);
        // Rejection sampling keeps the distribution uniform.
        if (b[0] as usize) < 252 {
            out.push(CHARS[b[0] as usize % 36] as char);
        }
    }
    out
}

// ─── Lookups scoped to the calling integration ───────────────────────────────

/// An org this integration manages, by the integrator's id for it.
pub async fn org_by_external(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    external_id: &str,
) -> Result<Uuid, ApiAppError> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM organizations WHERE integration_id = $1 AND external_id = $2",
    )
    .bind(caller.integration_id)
    .bind(external_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!(
            "Organization '{external_id}' not found"
        )))
    })
}

#[derive(Debug, Clone)]
pub struct AppRef {
    pub org_id: Uuid,
    pub app_id: Uuid,
    pub app_key: String,
}

pub async fn app_ref(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    org_external_id: &str,
    app_key: &str,
) -> Result<AppRef, ApiAppError> {
    let org_id = org_by_external(db, caller, org_external_id).await?;
    let app_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM integration_apps WHERE org_id = $1 AND app_key = $2",
    )
    .bind(org_id)
    .bind(app_key)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("App '{app_key}' not found"))))?;
    Ok(AppRef {
        org_id,
        app_id,
        app_key: app_key.to_string(),
    })
}

/// The project for one environment of an app.
pub async fn env_project(
    db: &sqlx::PgPool,
    app: &AppRef,
    environment: &str,
) -> Result<Uuid, ApiAppError> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM projects WHERE app_id = $1 AND environment = $2")
        .bind(app.app_id)
        .bind(environment)
        .fetch_optional(db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| {
            ApiAppError(AppError::NotFound(format!(
                "Environment '{environment}' of app '{}' not found",
                app.app_key
            )))
        })
}

/// Validate a caller-chosen identifier (external ids, app keys, environment
/// and service names): 1–63 chars of `a-z 0-9 -`, not starting or ending in `-`.
/// These end up in hostnames, Docker names and paths.
pub fn validate_slug(kind: &str, value: &str) -> Result<(), ApiAppError> {
    let ok = !value.is_empty()
        && value.len() <= 63
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err(ApiAppError(AppError::BadRequest(format!(
            "{kind} '{value}' must be 1-63 characters of a-z, 0-9 and '-', not starting or ending with '-'"
        ))))
    }
}

/// External ids come from the integrator's own database, so allow what ids
/// usually look like (uuids, cuids, nanoids) but nothing that needs escaping.
pub fn validate_external_id(value: &str) -> Result<(), ApiAppError> {
    let ok = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(ApiAppError(AppError::BadRequest(format!(
            "external id '{value}' must be 1-128 characters of A-Z, a-z, 0-9, '-' and '_'"
        ))))
    }
}

pub fn db_err(e: sqlx::Error) -> ApiAppError {
    ApiAppError(AppError::Database(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The integration router is nested inside the Open API's prefix; check
    /// both still route (axum panics at startup on conflicting routes).
    #[tokio::test]
    async fn integration_routes_mount_beside_openapi() {
        use axum::{body::Body, http::Request, routing::get};
        use tower::Service;
        let integration = Router::new().route("/orgs", get(|| async { "integration" }));
        let openapi = Router::new().route("/orgs", get(|| async { "openapi" }));
        let app = Router::new()
            .nest("/openapi/v1/integration", integration)
            .nest_service("/openapi/v1", openapi);
        for (path, want) in [
            ("/openapi/v1/integration/orgs", "integration"),
            ("/openapi/v1/orgs", "openapi"),
        ] {
            // Router is always ready, so calling without poll_ready is fine here.
            let resp = app
                .clone()
                .call(Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
            assert_eq!(body, want, "{path}");
        }
    }

    #[test]
    fn slugs() {
        assert!(validate_slug("app", "todo-app").is_ok());
        assert!(validate_slug("app", "a").is_ok());
        assert!(validate_slug("app", "").is_err());
        assert!(validate_slug("app", "-x").is_err());
        assert!(validate_slug("app", "Todo").is_err());
        assert!(validate_slug("app", "a_b").is_err());
        assert!(validate_slug("app", &"a".repeat(64)).is_err());
    }

    #[test]
    fn external_ids() {
        assert!(validate_external_id("org_2Nf8sK-1").is_ok());
        assert!(validate_external_id("0190f2a4-1b2c-7d3e-8f40-123456789abc").is_ok());
        assert!(validate_external_id("a b").is_err());
        assert!(validate_external_id("x'; drop").is_err());
    }

    #[test]
    fn randoms() {
        let a = random_alnum(24);
        assert_eq!(a.len(), 24);
        assert!(a
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()));
        assert_ne!(random_alnum(24), a);
        assert_eq!(random_hex(16).len(), 32);
    }
}
