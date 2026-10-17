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
pub mod domains;
pub mod keys;
pub mod orgs;
pub mod publish;
pub mod secrets;
pub mod transfer;
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
        .merge(domains::routes())
        .merge(transfer::routes())
        .merge(secrets::routes())
        .merge(webhooks::routes())
}

// ─── Caller ──────────────────────────────────────────────────────────────────

/// What a credential may reach. The integration API accepts three kinds of
/// credential, and each is confined to the orgs it legitimately owns:
///
/// - `Integration`: a platform `ship_int_` key. Only orgs whose
///   `organizations.integration_id` is this integration (created through
///   `PUT /orgs/{id}`, or linked by a superadmin with the claim endpoint).
/// - `OrgKey`: an organization `ship_` API key. Only its own org, and only
///   with the scopes the key was created with.
/// - `User`: a dashboard access token. Only orgs where the user is an owner or
///   admin (superadmins: any org).
///
/// An org that no integration has claimed is never reachable with an
/// integration key, and no credential reaches another tenant's org.
#[derive(Debug, Clone, PartialEq)]
pub enum Access {
    Integration { integration_id: Uuid },
    OrgKey { org_id: Uuid },
    User { user_id: Uuid },
}

#[derive(Debug, Clone)]
pub struct IntegrationCaller {
    pub access: Access,
    pub key_id: Uuid,
    pub key_name: String,
    pub scopes: Vec<String>,
}

/// Integration scopes an org API key earns from its own scopes. Deploying
/// alone doesn't allow creating apps or services; reading secret values needs
/// the org's `admin` scope.
pub fn scopes_for_org_key(key_scopes: &[String]) -> Vec<String> {
    let has = |s: &str| key_scopes.iter().any(|k| k == s);
    let mut out = vec![];
    if has("write") || has("admin") {
        out.push(SCOPE_MANAGE.to_string());
    }
    if has("admin") {
        out.push(SCOPE_REVEAL.to_string());
    }
    out
}

impl IntegrationCaller {
    /// The integration this caller belongs to. Operations that exist only at
    /// the platform level (creating orgs, webhooks) call this and are refused
    /// for org keys and dashboard users.
    pub fn integration_id(&self) -> Result<Uuid, ApiAppError> {
        match self.access {
            Access::Integration { integration_id } => Ok(integration_id),
            _ => Err(ApiAppError(AppError::Forbidden(
                "This operation needs an integration key ('ship_int_...')".into(),
            ))),
        }
    }

    pub fn is_integration(&self) -> bool {
        matches!(self.access, Access::Integration { .. })
    }

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
                access: Access::Integration { integration_id },
                key_id,
                key_name,
                scopes,
            });
        }

        // Organization API keys (`ship_...`): confined to the key's own org,
        // with the scopes the key was created with.
        if raw.starts_with("ship_") {
            let row = sqlx::query_as::<_, (Uuid, Uuid, String, Vec<String>)>(
                "SELECT k.id, k.org_id, k.name, k.scopes
                 FROM api_keys k
                 WHERE k.key_hash = $1 AND (k.expires_at IS NULL OR k.expires_at > NOW())",
            )
            .bind(hash_key(raw))
            .fetch_optional(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            if let Some((key_id, org_id, key_name, key_scopes)) = row {
                let db = state.db.clone();
                tokio::spawn(async move {
                    let _ = sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
                        .bind(key_id)
                        .execute(&db)
                        .await;
                });

                return Ok(IntegrationCaller {
                    access: Access::OrgKey { org_id },
                    key_id,
                    key_name,
                    scopes: scopes_for_org_key(&key_scopes),
                });
            }
        }

        // Dashboard access tokens (for trying the API from the web UI). Refresh
        // tokens are not accepted; orgs are limited to ones the user administers.
        if let Ok(claims) = crate::auth::decode_token(raw, &state.config.auth.jwt_secret) {
            if claims.token_type == "access" {
                if let Ok(user_id) = Uuid::parse_str(&claims.sub) {
                    return Ok(IntegrationCaller {
                        access: Access::User { user_id },
                        key_id: user_id,
                        key_name: claims.email,
                        scopes: vec![SCOPE_MANAGE.to_string(), SCOPE_REVEAL.to_string()],
                    });
                }
            }
        }

        Err(ApiAppError(AppError::Unauthorized(
            "Invalid authorization token. Provide an integration key ('ship_int_...'), an organization API key, or a dashboard access token.".into(),
        )))
    }
}

/// Deployment status as the integration API reports it: `running`,
/// `succeeded` or `failed`. Queued and pending deployments are `running`
/// (they will run); cancelled ones are `failed`. The raw status is also
/// returned as `raw_status`.
pub fn normalize_deploy_status(raw: &str) -> &'static str {
    match raw {
        "success" | "succeeded" => "succeeded",
        "failed" | "error" | "cancelled" => "failed",
        _ => "running",
    }
}

/// Step status in the same vocabulary. A step still `pending` after its
/// deployment finished never ran (an earlier step failed), so it is `skipped`.
pub fn normalize_step_status(raw: &str, deployment_status: &str) -> &'static str {
    match raw {
        "success" | "succeeded" => "succeeded",
        "failed" | "error" => "failed",
        "running" => "running",
        "skipped" => "skipped",
        _ if deployment_status != "running" => "skipped",
        _ => "pending",
    }
}

/// What a deployment step is for. `pull_image` is where source is built into
/// an image, so a failure there is a problem with the app's code or
/// Dockerfile, while a failure in `deploy` points at the platform or the
/// app not starting.
pub fn step_phase(name: &str) -> &'static str {
    match name {
        "validate_config" | "validate_compose_file" => "validate",
        "pull_image" | "clone_or_pull" | "build_site" => "build",
        "apply_env_vars" | "configure_volumes" | "configure_networks" | "configure_domains" => {
            "configure"
        }
        "create_or_update_service" | "go_live" | "publish_files" | "write_nginx_conf"
        | "docker_compose_up" => "deploy",
        "finalize" => "finalize",
        _ => "other",
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

/// Orgs the caller may act on, matching `identifier` as the integrator's
/// external id, the org's UUID, or its slug. Never returns an org the caller
/// has no claim on: see [`Access`].
pub async fn org_by_external(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    identifier: &str,
) -> Result<Uuid, ApiAppError> {
    let org_uuid = Uuid::parse_str(identifier).ok();
    let row = match &caller.access {
        Access::Integration { integration_id } => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM organizations
                 WHERE integration_id = $1 AND (external_id = $2 OR id = $3 OR slug = $2)
                 ORDER BY (external_id = $2) DESC LIMIT 1",
            )
            .bind(integration_id)
            .bind(identifier)
            .bind(org_uuid)
            .fetch_optional(db)
            .await
        }
        Access::OrgKey { org_id } => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM organizations
                 WHERE id = $1 AND (external_id = $2 OR id = $3 OR slug = $2)",
            )
            .bind(org_id)
            .bind(identifier)
            .bind(org_uuid)
            .fetch_optional(db)
            .await
        }
        Access::User { user_id } => {
            sqlx::query_scalar::<_, Uuid>(
                "SELECT o.id FROM organizations o
                 WHERE (o.external_id = $2 OR o.id = $3 OR o.slug = $2)
                   AND (EXISTS (SELECT 1 FROM org_members m
                                WHERE m.org_id = o.id AND m.user_id = $1
                                  AND m.role IN ('owner', 'admin'))
                        OR EXISTS (SELECT 1 FROM users u WHERE u.id = $1 AND u.is_superadmin))
                 ORDER BY (o.external_id = $2) DESC LIMIT 1",
            )
            .bind(user_id)
            .bind(identifier)
            .bind(org_uuid)
            .fetch_optional(db)
            .await
        }
    }
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // The same message whether the org doesn't exist or isn't the caller's,
    // so identifiers of other tenants' orgs can't be probed.
    row.ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!(
            "Organization '{identifier}' not found"
        )))
    })
}

/// Whether the caller may act on this org (same rules as [`org_by_external`]).
pub async fn can_access_org(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    org_id: Uuid,
) -> Result<bool, ApiAppError> {
    org_by_external(db, caller, &org_id.to_string())
        .await
        .map(|found| found == org_id)
        .or_else(|e| match e.0 {
            AppError::NotFound(_) => Ok(false),
            other => Err(ApiAppError(other)),
        })
}

/// Whether the caller may delete the org itself (not just its apps): an
/// integration key for its own orgs, an org owner, or a superadmin. Org keys
/// never delete their own org.
pub async fn can_delete_org(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    org_id: Uuid,
) -> Result<bool, ApiAppError> {
    match &caller.access {
        Access::Integration { .. } => can_access_org(db, caller, org_id).await,
        Access::OrgKey { .. } => Ok(false),
        Access::User { user_id } => sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM org_members m WHERE m.org_id = $1 AND m.user_id = $2 AND m.role = 'owner')
                 OR EXISTS (SELECT 1 FROM users u WHERE u.id = $2 AND u.is_superadmin)",
        )
        .bind(org_id)
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string()))),
    }
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

    // Legacy: adopt a project nobody owns yet (made in the dashboard). A project
    // that already belongs to another app is never returned for this key.
    let project_row = sqlx::query_as::<_, (Uuid, Option<Uuid>)>(
        "SELECT id, app_id FROM projects
         WHERE org_id = $1 AND app_id IS NULL AND (slug = $2 OR id = $3)
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
         WHERE app_id = $1 AND (environment = $2 OR id = $3)",
    )
    .bind(app.app_id)
    .bind(environment)
    .bind(env_uuid)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    row.ok_or_else(|| {
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
mod access_tests {
    //! The cross-tenant scenario, against a real database. Needs
    //! `TEST_DATABASE_URL` (a role that can create databases) and runs with
    //! `cargo test -- --include-ignored`; skipped when it isn't set.

    use super::*;
    use sqlx::postgres::PgPoolOptions;

    fn caller(access: Access, scopes: &[&str]) -> IntegrationCaller {
        IntegrationCaller {
            access,
            key_id: Uuid::now_v7(),
            key_name: "test".into(),
            scopes: scopes.iter().map(|s| s.to_string()).collect(),
        }
    }

    async fn fresh_db() -> Option<(sqlx::PgPool, sqlx::PgPool, String)> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let admin = PgPoolOptions::new().max_connections(2).connect(&url).await.ok()?;
        let name = format!("access_test_{}", random_alnum(10));
        sqlx::query(&format!("CREATE DATABASE {name}")).execute(&admin).await.ok()?;
        let (base, _) = url.rsplit_once('/')?;
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&format!("{base}/{name}"))
            .await
            .ok()?;
        shipyard_db::run_migrations(&pool).await.ok()?;
        Some((admin, pool, name))
    }

    async fn org(db: &sqlx::PgPool, slug: &str, integration: Option<Uuid>, ext: Option<&str>) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO organizations (id, name, slug, integration_id, external_id)
             VALUES ($1, $2, $2, $3, $4) RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(slug)
        .bind(integration)
        .bind(ext)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn user(db: &sqlx::PgPool, email: &str, superadmin: bool) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO users (id, email, password_hash, is_superadmin) VALUES ($1, $2, 'x', $3) RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(email)
        .bind(superadmin)
        .fetch_one(db)
        .await
        .unwrap()
    }

    async fn member(db: &sqlx::PgPool, org: Uuid, user: Uuid, role: &str) {
        sqlx::query("INSERT INTO org_members (id, org_id, user_id, role) VALUES ($1, $2, $3, $4::member_role)")
            .bind(Uuid::now_v7())
            .bind(org)
            .bind(user)
            .bind(role)
            .execute(db)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn credentials_only_reach_their_own_orgs() {
        let Some((admin, db, name)) = fresh_db().await else {
            eprintln!("skipped: set TEST_DATABASE_URL");
            return;
        };

        let (i1, i2) = (Uuid::now_v7(), Uuid::now_v7());
        for (id, n) in [(i1, "nomi"), (i2, "other")] {
            sqlx::query("INSERT INTO integrations (id, name) VALUES ($1, $2)")
                .bind(id)
                .bind(n)
                .execute(&db)
                .await
                .unwrap();
        }
        let owned = org(&db, "owned-by-nomi", Some(i1), Some("nomi_org_1")).await;
        let other_integration = org(&db, "owned-by-other", Some(i2), Some("o1")).await;
        let victim = org(&db, "victim", None, None).await; // created in the dashboard
        let mallory_org = org(&db, "mallory-co", None, None).await;

        let alice = user(&db, "alice@x.test", false).await;
        let bob = user(&db, "bob@x.test", false).await;
        let mallory = user(&db, "mallory@x.test", false).await;
        let root = user(&db, "root@x.test", true).await;
        member(&db, victim, alice, "owner").await;
        member(&db, victim, bob, "member").await;
        let carol = user(&db, "carol@x.test", false).await;
        member(&db, victim, carol, "admin").await;
        member(&db, mallory_org, mallory, "owner").await;

        let find = |c: &IntegrationCaller, id: String| {
            let db = db.clone();
            let c = c.clone();
            async move { org_by_external(&db, &c, &id).await.ok() }
        };

        // Integration key: its own orgs by external id, slug or uuid; nothing else.
        let nomi = caller(Access::Integration { integration_id: i1 }, &["manage"]);
        assert_eq!(find(&nomi, "nomi_org_1".into()).await, Some(owned));
        assert_eq!(find(&nomi, "owned-by-nomi".into()).await, Some(owned));
        assert_eq!(find(&nomi, owned.to_string()).await, Some(owned));
        assert_eq!(find(&nomi, "victim".into()).await, None, "unclaimed dashboard org");
        assert_eq!(find(&nomi, victim.to_string()).await, None);
        assert_eq!(find(&nomi, "owned-by-other".into()).await, None, "another integration's org");
        assert_eq!(find(&nomi, other_integration.to_string()).await, None);

        // Org API key: only its own org, whatever the identifier.
        let key = caller(Access::OrgKey { org_id: mallory_org }, &["manage"]);
        assert_eq!(find(&key, "mallory-co".into()).await, Some(mallory_org));
        assert_eq!(find(&key, "victim".into()).await, None);
        assert_eq!(find(&key, victim.to_string()).await, None);
        assert_eq!(find(&key, "owned-by-nomi".into()).await, None);

        // Dashboard users: orgs they own or administer; superadmin: all.
        let as_user = |u| caller(Access::User { user_id: u }, &["manage", "secrets:reveal"]);
        assert_eq!(find(&as_user(mallory), "mallory-co".into()).await, Some(mallory_org));
        assert_eq!(find(&as_user(mallory), "victim".into()).await, None, "the exploit");
        assert_eq!(find(&as_user(mallory), victim.to_string()).await, None);
        assert_eq!(find(&as_user(alice), "victim".into()).await, Some(victim));
        assert_eq!(find(&as_user(bob), "victim".into()).await, None, "plain members don't manage");
        assert_eq!(find(&as_user(carol), "victim".into()).await, Some(victim), "admins manage");
        assert_eq!(find(&as_user(root), "victim".into()).await, Some(victim));
        assert_eq!(find(&as_user(root), "owned-by-nomi".into()).await, Some(owned));

        // Deleting an org is narrower still.
        assert!(can_delete_org(&db, &nomi, owned).await.unwrap());
        assert!(!can_delete_org(&db, &nomi, victim).await.unwrap());
        assert!(!can_delete_org(&db, &key, mallory_org).await.unwrap(), "org keys can't delete their org");
        assert!(can_delete_org(&db, &as_user(alice), victim).await.unwrap());
        assert!(!can_delete_org(&db, &as_user(bob), victim).await.unwrap());
        assert!(!can_delete_org(&db, &as_user(carol), victim).await.unwrap(), "admins can't delete the org");
        assert!(can_delete_org(&db, &as_user(root), victim).await.unwrap());

        // can_access_org follows the same rules (used by secret reveal).
        assert!(can_access_org(&db, &nomi, owned).await.unwrap());
        assert!(!can_access_org(&db, &nomi, victim).await.unwrap());
        assert!(!can_access_org(&db, &as_user(mallory), victim).await.unwrap());

        // An explicit claim (the admin endpoint's UPDATE) is what grants access.
        sqlx::query("UPDATE organizations SET integration_id = $1, external_id = 'victim' WHERE id = $2")
            .bind(i1)
            .bind(victim)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(find(&nomi, "victim".into()).await, Some(victim));

        db.close().await;
        let _ = sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)")).execute(&admin).await;
    }

    /// One app's environment must never resolve to another app's project, even
    /// when the requested app has no such environment or shares a name.
    #[tokio::test]
    #[ignore]
    async fn environments_never_resolve_to_another_apps_project() {
        let Some((admin, db, name)) = fresh_db().await else {
            eprintln!("skipped: set TEST_DATABASE_URL");
            return;
        };
        let integ = Uuid::now_v7();
        sqlx::query("INSERT INTO integrations (id, name) VALUES ($1, 'nomi')")
            .bind(integ)
            .execute(&db)
            .await
            .unwrap();
        let org_id = org(&db, "acme", Some(integ), Some("org_1")).await;

        let mut apps = vec![];
        for key in ["todo", "habit"] {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO integration_apps (id, org_id, app_key, name) VALUES ($1, $2, $3, $3) RETURNING id",
            )
            .bind(Uuid::now_v7())
            .bind(org_id)
            .bind(key)
            .fetch_one(&db)
            .await
            .unwrap();
            apps.push(id);
        }
        // Only "todo" has a production environment.
        let todo_prod: Uuid = sqlx::query_scalar(
            "INSERT INTO projects (id, org_id, name, slug, directory_path, app_id, environment)
             VALUES ($1, $2, 'todo-production', 'todo-production', '/x', $3, 'production') RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(org_id)
        .bind(apps[0])
        .fetch_one(&db)
        .await
        .unwrap();

        let ref_for = |app_id: Uuid, key: &str| AppRef { org_id, app_id, app_key: key.into() };

        assert_eq!(env_project(&db, &ref_for(apps[0], "todo"), "production").await.ok(), Some(todo_prod));
        assert!(env_project(&db, &ref_for(apps[1], "habit"), "production").await.is_err());
        // Neither slug, name nor the other app's project id leak across apps.
        assert!(env_project(&db, &ref_for(apps[1], "habit"), "todo-production").await.is_err());
        assert!(env_project(&db, &ref_for(apps[1], "habit"), &todo_prod.to_string()).await.is_err());

        // A key that isn't an app must not adopt another app's project.
        let c = caller(Access::Integration { integration_id: integ }, &["manage"]);
        assert!(app_ref(&db, &c, "org_1", "todo-production").await.is_err());
        assert!(app_ref(&db, &c, "org_1", &todo_prod.to_string()).await.is_err());

        db.close().await;
        let _ = sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)")).execute(&admin).await;
    }

    /// An app moves to another org with everything it owns, in one step, and
    /// is refused cleanly when the destination can't take it.
    #[tokio::test]
    #[ignore]
    async fn transferring_an_app_moves_everything_it_owns() {
        let Some((admin, db, name)) = fresh_db().await else {
            eprintln!("skipped: set TEST_DATABASE_URL");
            return;
        };
        let integ = Uuid::now_v7();
        sqlx::query("INSERT INTO integrations (id, name) VALUES ($1, 'nomi')")
            .bind(integ)
            .execute(&db)
            .await
            .unwrap();
        let (a, b) = (
            org(&db, "org-a", Some(integ), Some("org_a")).await,
            org(&db, "org-b", Some(integ), Some("org_b")).await,
        );
        let app: Uuid = sqlx::query_scalar(
            "INSERT INTO integration_apps (id, org_id, app_key, name) VALUES ($1, $2, 'todo', 'Todo') RETURNING id",
        )
        .bind(Uuid::now_v7())
        .bind(a)
        .fetch_one(&db)
        .await
        .unwrap();
        let mut projects = vec![];
        for env in ["production", "staging"] {
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO projects (id, org_id, name, slug, directory_path, app_id, environment)
                 VALUES ($1, $2, $3, $3, '/x', $4, $5) RETURNING id",
            )
            .bind(Uuid::now_v7())
            .bind(a)
            .bind(format!("todo-{env}"))
            .bind(app)
            .bind(env)
            .fetch_one(&db)
            .await
            .unwrap();
            sqlx::query("INSERT INTO publishes (id, org_id, project_id, status, manifest) VALUES ($1, $2, $3, 'succeeded', '{}')")
                .bind(Uuid::now_v7())
                .bind(a)
                .bind(id)
                .execute(&db)
                .await
                .unwrap();
            projects.push(id);
        }
        // The destination already uses one of the project slugs.
        sqlx::query("INSERT INTO projects (id, org_id, name, slug, directory_path) VALUES ($1, $2, 'x', 'todo-staging', '/y')")
            .bind(Uuid::now_v7())
            .bind(b)
            .execute(&db)
            .await
            .unwrap();

        // A running publish blocks the move.
        sqlx::query("UPDATE publishes SET status = 'running' WHERE project_id = $1")
            .bind(projects[0])
            .execute(&db)
            .await
            .unwrap();
        assert!(matches!(
            transfer::move_app(&db, app, a, b, None).await,
            Err(transfer::MoveError::Conflict(_))
        ));
        sqlx::query("UPDATE publishes SET status = 'succeeded'").execute(&db).await.unwrap();

        let moved = transfer::move_app(&db, app, a, b, None).await.unwrap();
        assert_eq!(moved.app_key, "todo");
        assert_eq!(moved.environments.len(), 2);

        let owners: Vec<Uuid> = sqlx::query_scalar("SELECT org_id FROM projects WHERE app_id = $1")
            .bind(app)
            .fetch_all(&db)
            .await
            .unwrap();
        assert!(owners.iter().all(|o| *o == b));
        let pub_owner: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT org_id FROM publishes")
            .fetch_all(&db)
            .await
            .unwrap();
        assert_eq!(pub_owner, vec![b]);
        // The clashing slug was renamed, the other kept.
        let slugs: Vec<String> = moved.environments.iter().map(|e| e.2.clone()).collect();
        assert!(slugs.contains(&"todo-production".to_string()));
        assert!(slugs.iter().any(|s| s.starts_with("todo-staging-") && s != "todo-staging"));
        // The app is reachable under the new org only.
        let c = caller(Access::Integration { integration_id: integ }, &["manage"]);
        assert!(app_ref(&db, &c, "org_b", "todo").await.is_ok());
        assert!(app_ref(&db, &c, "org_a", "todo").await.is_err());

        // Moving back is refused when the key is taken in the destination.
        sqlx::query("INSERT INTO integration_apps (id, org_id, app_key, name) VALUES ($1, $2, 'todo', 'Other')")
            .bind(Uuid::now_v7())
            .bind(a)
            .execute(&db)
            .await
            .unwrap();
        assert!(matches!(
            transfer::move_app(&db, app, b, a, None).await,
            Err(transfer::MoveError::Conflict(_))
        ));
        assert!(transfer::move_app(&db, app, b, a, Some("todo-2")).await.is_ok());

        db.close().await;
        let _ = sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)")).execute(&admin).await;
    }

}

#[cfg(test)]
mod scope_tests {
    use super::*;

    #[test]
    fn steps_that_never_ran_are_skipped_not_running() {
        assert_eq!(normalize_step_status("pending", "failed"), "skipped");
        assert_eq!(normalize_step_status("pending", "succeeded"), "skipped");
        assert_eq!(normalize_step_status("pending", "running"), "pending");
        assert_eq!(normalize_step_status("success", "failed"), "succeeded");
        assert_eq!(normalize_step_status("failed", "failed"), "failed");
        assert_eq!(normalize_step_status("running", "running"), "running");
        assert_eq!(step_phase("pull_image"), "build");
        assert_eq!(step_phase("create_or_update_service"), "deploy");
    }

    #[test]
    fn deployment_statuses_use_one_vocabulary() {
        for (raw, want) in [
            ("success", "succeeded"),
            ("succeeded", "succeeded"),
            ("failed", "failed"),
            ("cancelled", "failed"),
            ("running", "running"),
            ("pending", "running"),
            ("queued", "running"),
        ] {
            assert_eq!(normalize_deploy_status(raw), want, "{raw}");
        }
    }

    #[test]
    fn org_key_scopes_map_conservatively() {
        let s = |v: &[&str]| scopes_for_org_key(&v.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert!(s(&["read"]).is_empty());
        assert!(s(&["deploy"]).is_empty(), "deploy-only keys manage nothing");
        assert_eq!(s(&["write"]), vec!["manage"]);
        assert_eq!(s(&["admin"]), vec!["manage", "secrets:reveal"]);
        assert_eq!(s(&["deploy", "write"]), vec!["manage"]);
    }
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

            // Checking deployments and publishes, and the operations on them
            (Method::GET, "/orgs/my-org/deployments/01a13ce7-da0c-7c11-a1ec-a6ffb8b74505"),
            (Method::GET, "/deployments/01a13ce7-da0c-7c11-a1ec-a6ffb8b74505"),
            (Method::GET, "/orgs/my-org/apps/my-app/envs/production/deployments"),
            (Method::GET, "/orgs/my-org/publishes/01a13ce7-da0c-7c11-a1ec-a6ffb8b74505"),
            (Method::GET, "/orgs/my-org/apps/my-app/publishes"),
            (Method::POST, "/orgs/my-org/apps/my-app/envs/production/services/web/rollback"),
            (Method::POST, "/orgs/my-org/apps/my-app/envs/production/restart"),
            (Method::GET, "/orgs/my-org/apps/my-app/envs/production/services/web/env"),
            (Method::POST, "/orgs/my-org/apps/my-app/envs/production/duplicate"),
            (Method::GET, "/orgs/my-org/domains/check?subdomain=shop"),
            (Method::POST, "/orgs/my-org/apps/my-app/transfer"),
            (Method::GET, "/orgs/my-org/apps/my-app/envs/production/domain"),
            (Method::PUT, "/orgs/my-org/apps/my-app/envs/production/domain"),

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
