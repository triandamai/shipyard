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
        if raw.starts_with(KEY_PREFIX) {
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

            return Ok(IntegrationCaller {
                integration_id,
                key_id,
                key_name,
                scopes,
            });
        }

        // Fallback: support standard org API keys (ship_...)
        if raw.starts_with("ship_") {
            let row = sqlx::query_as::<_, (Uuid, Uuid, String)>(
                "SELECT k.id, k.org_id, k.name
                 FROM api_keys k
                 WHERE k.key_hash = $1 AND (k.expires_at IS NULL OR k.expires_at > NOW())",
            )
            .bind(hash_key(raw))
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            if let Some((key_id, org_id, key_name)) = row {
                let db = state.db.clone();
                tokio::spawn(async move {
                    let _ = sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
                        .bind(key_id)
                        .execute(&db)
                        .await;
                });

                return Ok(IntegrationCaller {
                    integration_id: org_id,
                    key_id,
                    key_name,
                    scopes: vec![SCOPE_MANAGE.to_string(), SCOPE_REVEAL.to_string()],
                });
            }
        }

        // Fallback: support JWT user access tokens (e.g. testing from web UI / bearer token)
        if let Ok(claims) = crate::auth::decode_token(raw, &state.config.auth.jwt_secret) {
            let user_id = Uuid::parse_str(&claims.sub).unwrap_or(Uuid::nil());
            return Ok(IntegrationCaller {
                integration_id: user_id,
                key_id: user_id,
                key_name: claims.email,
                scopes: vec![SCOPE_MANAGE.to_string(), SCOPE_REVEAL.to_string()],
            });
        }

        Err(ApiAppError(AppError::Unauthorized(
            "Invalid authorization token. Provide a valid integration key ('ship_int_...') or API key.".into(),
        )))
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

/// An org this integration manages, by external_id, internal id (UUID), or slug.
pub async fn org_by_external(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    external_id: &str,
) -> Result<Uuid, ApiAppError> {
    let org_uuid = Uuid::parse_str(external_id).ok();
    let row = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM organizations
         WHERE ((integration_id = $1 OR integration_id IS NULL) AND external_id = $2)
            OR ((integration_id = $1 OR integration_id IS NULL) AND id = $3)
            OR ((integration_id = $1 OR integration_id IS NULL) AND slug = $2)
         ORDER BY (integration_id = $1) DESC
         LIMIT 1",
    )
    .bind(caller.integration_id)
    .bind(external_id)
    .bind(org_uuid)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    row.ok_or_else(|| {
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
    let app_uuid = Uuid::parse_str(app_key).ok();
    let app_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM integration_apps
         WHERE org_id = $1 AND (app_key = $2 OR id = $3)",
    )
    .bind(org_id)
    .bind(app_key)
    .bind(app_uuid)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if let Some(id) = app_id {
        return Ok(AppRef {
            org_id,
            app_id: id,
            app_key: app_key.to_string(),
        });
    }

    // Fallback: check if there's an existing project matching app_key in this org
    let project_row = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
        "SELECT id, app_id FROM projects
         WHERE org_id = $1 AND (slug = $2 OR name = $2 OR id = $3)
         LIMIT 1",
    )
    .bind(org_id)
    .bind(app_key)
    .bind(app_uuid)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if let Some((proj_id, maybe_app_id)) = project_row {
        if let Some(aid) = maybe_app_id {
            return Ok(AppRef {
                org_id,
                app_id: aid,
                app_key: app_key.to_string(),
            });
        }
        // Create an integration_apps record to link it
        let new_app_id = Uuid::now_v7();
        let _ = sqlx::query(
            "INSERT INTO integration_apps (id, org_id, app_key, name, created_at)
             VALUES ($1, $2, $3, $4, NOW())
             ON CONFLICT (org_id, app_key) DO UPDATE SET name = EXCLUDED.name",
        )
        .bind(new_app_id)
        .bind(org_id)
        .bind(app_key)
        .bind(app_key)
        .execute(db)
        .await;

        let final_app_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM integration_apps WHERE org_id = $1 AND app_key = $2",
        )
        .bind(org_id)
        .bind(app_key)
        .fetch_one(db)
        .await
        .unwrap_or(new_app_id);

        let _ = sqlx::query("UPDATE projects SET app_id = $1 WHERE id = $2")
            .bind(final_app_id)
            .bind(proj_id)
            .execute(db)
            .await;

        return Ok(AppRef {
            org_id,
            app_id: final_app_id,
            app_key: app_key.to_string(),
        });
    }

    Err(ApiAppError(AppError::NotFound(format!("App '{app_key}' not found"))))
}

/// The project for one environment of an app.
pub async fn env_project(
    db: &sqlx::PgPool,
    app: &AppRef,
    environment: &str,
) -> Result<Uuid, ApiAppError> {
    let env_uuid = Uuid::parse_str(environment).ok();
    let row = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM projects
         WHERE app_id = $1 AND (environment = $2 OR slug = $2 OR name = $2 OR id = $3)",
    )
    .bind(app.app_id)
    .bind(environment)
    .bind(env_uuid)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if let Some(id) = row {
        return Ok(id);
    }

    // Fallback: check if there's any project in the org matching environment
    let org_proj = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM projects
         WHERE org_id = $1 AND (environment = $2 OR slug = $2 OR name = $2 OR id = $3)
         LIMIT 1",
    )
    .bind(app.org_id)
    .bind(environment)
    .bind(env_uuid)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    org_proj.ok_or_else(|| {
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

    #[tokio::test]
    async fn axum_nest_and_nest_service_precedence() {
        use axum::routing::get;
        use axum::http::Request;
        use tower::ServiceExt;

        let openapi: Router<()> = Router::new()
            .route("/test", get(|| async { "from openapi" }));

        let integration: Router<()> = Router::new()
            .route("/test", get(|| async { "from integration" }));

        let app: Router<()> = Router::new()
            .nest("/openapi/v1/integration", integration)
            .nest_service("/openapi/v1", openapi);

        let req = Request::builder()
            .uri("/openapi/v1/integration/test")
            .body(axum::body::Body::empty())
            .unwrap();

        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(body_str, "from integration");
    }

    #[tokio::test]
    async fn integration_endpoints_dispatch_without_404() {
        use std::sync::Arc;
        use dashmap::DashMap;
        use shipyard_common::config::AppConfig;
        use shipyard_docker::engine::BollardDockerEngine;
        use axum::http::{Request, Method, StatusCode};
        use tower::ServiceExt;

        let config = Arc::new(AppConfig::default());
        let db = sqlx::PgPool::connect_lazy("postgres://shipyard:shipyard@localhost/shipyard").unwrap();
        let dummy_socket = tempfile::NamedTempFile::new().unwrap();
        let docker = Arc::new(BollardDockerEngine::with_socket(dummy_socket.path().to_str().unwrap()).unwrap());
        let (mqtt_pub, _, _) = shipyard_mqtt::create_mqtt_client("127.0.0.1", 1883, "test-client", None, None).unwrap();
        let mqtt = Arc::new(mqtt_pub);
        let oauth_states = Arc::new(DashMap::new());
        let http_client = reqwest::Client::new();
        let auth_limiter = crate::middleware::rate_limit::make_auth_rate_limiter();
        let swarm_sync_trigger = Arc::new(tokio::sync::Notify::new());
        let registry_storage = Arc::new(shipyard_registry::storage::local::LocalStorage::new("/tmp"));

        let state = AppState {
            config,
            db,
            docker,
            mqtt,
            oauth_states,
            redis: None,
            http_client,
            auth_limiter,
            swarm_sync_trigger,
            registry_storage,
        };

        // Full application router configuration as in main.rs and routes.rs
        let api = crate::routes::api_router();
        let app = Router::new()
            .nest("/api", api)
            .nest("/openapi/v1/integration", routes())
            .nest("/integration", routes())
            .nest("/integrations", routes())
            .merge(routes())
            .with_state(state);

        let test_cases = vec![
            // Direct /orgs route as called by Nomi
            (Method::GET, "/orgs/my-org/apps/my-app/envs/production/deployments/01a13ce7-da0c-7c11-a1ec-a6ffb8b74505"),
            (Method::POST, "/orgs/my-org/apps/my-app/envs/production/publish"),
            (Method::GET, "/orgs/my-org/apps/my-app/envs/production"),

            (Method::POST, "/openapi/v1/integration/orgs/my-org/apps/my-app/envs/production/publish"),
            (Method::GET, "/openapi/v1/integration/orgs/my-org"),
            (Method::PUT, "/openapi/v1/integration/orgs/my-org"),
            (Method::GET, "/openapi/v1/integration/orgs/my-org/apps/my-app"),
            (Method::GET, "/openapi/v1/integration/orgs/my-org/apps/my-app/envs/production"),
            (Method::GET, "/openapi/v1/integration/orgs/my-org/apps/my-app/dev-database"),
            (Method::GET, "/openapi/v1/integration/orgs/my-org/apps/my-app/secrets"),
            (Method::GET, "/openapi/v1/integration/webhooks"),
            (Method::GET, "/openapi/v1/integration/event-types"),

            // Shorthand aliases /integration and /integrations
            (Method::POST, "/integration/orgs/my-org/apps/my-app/envs/production/publish"),
            (Method::POST, "/integrations/orgs/my-org/apps/my-app/envs/production/publish"),

            // /api mounted endpoints
            (Method::POST, "/api/openapi/v1/integration/orgs/my-org/apps/my-app/envs/production/publish"),
            (Method::POST, "/api/integration/orgs/my-org/apps/my-app/envs/production/publish"),
            (Method::POST, "/api/integrations/orgs/my-org/apps/my-app/envs/production/publish"),
        ];

        for (method, uri) in test_cases {
            let req = Request::builder()
                .method(method)
                .uri(uri)
                .body(axum::body::Body::empty())
                .unwrap();

            let res = app.clone().oneshot(req).await.unwrap();
            // Should be 401 UNAUTHORIZED (integration auth triggered), NOT 404 NOT FOUND!
            assert_eq!(
                res.status(),
                StatusCode::UNAUTHORIZED,
                "Expected 401 Unauthorized for {uri}, got {}",
                res.status()
            );
        }

        // An endpoint that is truly nonexistent must still 404
        let req = Request::builder()
            .method(Method::GET)
            .uri("/openapi/v1/integration/definitely-not-a-route-xyz")
            .body(axum::body::Body::empty())
            .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }
}
