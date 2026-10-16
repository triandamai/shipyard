//! The secret store.
//!
//! Values are AES-256-GCM encrypted with the platform secret key. Outside the
//! reveal endpoint, a secret is only ever referred to by its `ref`
//! (`sec_<32 hex>`), so integrators can pass refs around (to agents, logs,
//! UIs) without exposing values. Revealing needs the `secrets:reveal` scope
//! and is audit-logged.
//!
//! Secrets are scoped to an app, optionally narrowed to one environment
//! (project). A service env var bound to a secret (`secret_bindings`) is
//! updated whenever the secret changes; the service picks it up on its next
//! deploy or restart.

use axum::{
    extract::{Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::crypto;
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events::{self, Scope};

use super::{
    app_ref, db_err, env_project, random_alnum, random_hex, IntegrationCaller, SCOPE_MANAGE,
    SCOPE_REVEAL,
};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org/apps/:app/secrets", get(list_secrets))
        .route(
            "/orgs/:org/apps/:app/secrets/:name",
            put(set_secret).delete(delete_secret),
        )
        .route("/secrets/:secret_ref/reveal", post(reveal_secret))
}

// ─── Store functions (used by dev databases and publish too) ─────────────────

#[derive(Debug, Clone)]
pub struct SecretHandle {
    pub id: Uuid,
    pub secret_ref: String,
}

/// How to generate a value when the caller doesn't supply one.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Generate {
    /// 32 chars of a-z0-9 (~165 bits); safe in URLs and connection strings.
    Password,
    /// 32 random bytes as 64 hex chars, e.g. session or JWT secrets.
    Hex32,
    /// 32 random bytes, base64url without padding.
    Base64_32,
    Uuid,
}

pub fn generate(kind: Generate) -> String {
    use base64::Engine;
    match kind {
        Generate::Password => random_alnum(32),
        Generate::Hex32 => random_hex(32),
        Generate::Base64_32 => {
            let bytes = hex::decode(random_hex(32)).unwrap_or_default();
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
        }
        Generate::Uuid => Uuid::new_v4().to_string(),
    }
}

/// Create or replace a secret's value. Returns its handle (the ref is stable
/// across updates).
#[allow(clippy::too_many_arguments)]
pub async fn upsert_secret(
    db: &sqlx::PgPool,
    secret_key: &str,
    org_id: Uuid,
    app_id: Option<Uuid>,
    project_id: Option<Uuid>,
    name: &str,
    kind: &str,
    value: &str,
) -> Result<SecretHandle, ApiAppError> {
    let encrypted = crypto::encrypt(secret_key, value).map_err(ApiAppError)?;
    let (id, secret_ref): (Uuid, String) = sqlx::query_as(
        r#"INSERT INTO secrets (id, ref, org_id, app_id, project_id, name, kind, value_encrypted)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
           ON CONFLICT (org_id, COALESCE(app_id, '00000000-0000-0000-0000-000000000000'::uuid),
                        COALESCE(project_id, '00000000-0000-0000-0000-000000000000'::uuid), name)
           DO UPDATE SET value_encrypted = EXCLUDED.value_encrypted, kind = EXCLUDED.kind, updated_at = NOW()
           RETURNING id, ref"#,
    )
    .bind(Uuid::now_v7())
    .bind(format!("sec_{}", random_hex(16)))
    .bind(org_id)
    .bind(app_id)
    .bind(project_id)
    .bind(name)
    .bind(kind)
    .bind(&encrypted)
    .fetch_one(db)
    .await
    .map_err(db_err)?;

    // Push the new value into every env var bound to this secret.
    sqlx::query(
        "UPDATE service_envs se SET value_encrypted = $2
         FROM secret_bindings b
         WHERE b.secret_id = $1 AND se.service_id = b.service_id AND se.key = b.env_key",
    )
    .bind(id)
    .bind(&encrypted)
    .execute(db)
    .await
    .map_err(db_err)?;

    Ok(SecretHandle { id, secret_ref })
}

/// Decrypted value of a secret by id. Server-side use only (building
/// connection strings, resolving env vars); never returned to a caller
/// without the reveal scope.
pub async fn secret_value(
    db: &sqlx::PgPool,
    secret_key: &str,
    id: Uuid,
) -> Result<String, ApiAppError> {
    let enc: String = sqlx::query_scalar("SELECT value_encrypted FROM secrets WHERE id = $1")
        .bind(id)
        .fetch_optional(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| ApiAppError(AppError::NotFound("Secret not found".into())))?;
    crypto::decrypt(secret_key, &enc).map_err(ApiAppError)
}

/// Find a secret visible to an environment: the env-specific one wins over the
/// app-wide one.
pub async fn find_secret(
    db: &sqlx::PgPool,
    app_id: Uuid,
    project_id: Option<Uuid>,
    name: &str,
) -> Result<Option<Uuid>, ApiAppError> {
    sqlx::query_scalar(
        "SELECT id FROM secrets
         WHERE app_id = $1 AND name = $3 AND (project_id = $2 OR project_id IS NULL)
         ORDER BY project_id NULLS LAST LIMIT 1",
    )
    .bind(app_id)
    .bind(project_id)
    .bind(name)
    .fetch_optional(db)
    .await
    .map_err(db_err)
}

/// Set a service env var from a secret and remember the binding.
pub async fn bind_env(
    db: &sqlx::PgPool,
    service_id: Uuid,
    env_key: &str,
    secret_id: Uuid,
) -> Result<(), ApiAppError> {
    sqlx::query(
        r#"INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
           SELECT $1, $2, $3, s.value_encrypted, TRUE, NOW() FROM secrets s WHERE s.id = $4
           ON CONFLICT (service_id, key) DO UPDATE
             SET value_encrypted = EXCLUDED.value_encrypted, is_secret = TRUE"#,
    )
    .bind(Uuid::now_v7())
    .bind(service_id)
    .bind(env_key)
    .bind(secret_id)
    .execute(db)
    .await
    .map_err(db_err)?;
    sqlx::query(
        "INSERT INTO secret_bindings (service_id, env_key, secret_id) VALUES ($1, $2, $3)
         ON CONFLICT (service_id, env_key) DO UPDATE SET secret_id = EXCLUDED.secret_id",
    )
    .bind(service_id)
    .bind(env_key)
    .bind(secret_id)
    .execute(db)
    .await
    .map_err(db_err)?;
    Ok(())
}

// ─── Routes ──────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct SecretOut {
    #[serde(rename = "ref")]
    secret_ref: String,
    name: String,
    kind: String,
    /// `null` = app-wide; otherwise the environment it applies to.
    environment: Option<String>,
    used_by: Vec<String>,
    updated_at: DateTime<Utc>,
    last_revealed_at: Option<DateTime<Utc>>,
}

/// GET /orgs/:org/apps/:app/secrets — names and refs, never values.
async fn list_secrets(
    caller: IntegrationCaller,
    Path((org, app)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<SecretOut>>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app).await?;
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            Option<String>,
            DateTime<Utc>,
            Option<DateTime<Utc>>,
        ),
    >(
        "SELECT s.id, s.ref, s.name, s.kind, p.environment, s.updated_at, s.last_revealed_at
         FROM secrets s LEFT JOIN projects p ON p.id = s.project_id
         WHERE s.app_id = $1 ORDER BY s.name, p.environment NULLS FIRST",
    )
    .bind(app.app_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let mut out = Vec::with_capacity(rows.len());
    for (id, secret_ref, name, kind, environment, updated_at, last_revealed_at) in rows {
        let used_by: Vec<String> = sqlx::query_scalar(
            "SELECT p.environment || '/' || s.slug || ':' || b.env_key
             FROM secret_bindings b JOIN services s ON s.id = b.service_id JOIN projects p ON p.id = s.project_id
             WHERE b.secret_id = $1 ORDER BY 1",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;
        out.push(SecretOut {
            secret_ref,
            name,
            kind,
            environment,
            used_by,
            updated_at,
            last_revealed_at,
        });
    }
    Ok(Json(ApiResponse::ok(out)))
}

#[derive(Deserialize)]
struct SetSecret {
    /// The value. Omit it and set `generate` to have Shipyard make one.
    value: Option<String>,
    generate: Option<Generate>,
    /// Limit to one environment; omit for app-wide.
    environment: Option<String>,
}

/// PUT /orgs/:org/apps/:app/secrets/:name — create or change a secret. The
/// response has the ref, never the value.
async fn set_secret(
    caller: IntegrationCaller,
    Path((org, app, name)): Path<(String, String, String)>,
    State(state): State<AppState>,
    Json(body): Json<SetSecret>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_env_name(&name)?;
    let app = app_ref(&state.db, &caller, &org, &app).await?;
    let project_id = match &body.environment {
        Some(env) => Some(env_project(&state.db, &app, env).await?),
        None => None,
    };
    let value = match (body.value, body.generate) {
        (Some(v), None) => v,
        (None, Some(g)) => generate(g),
        _ => {
            return Err(ApiAppError(AppError::BadRequest(
                "Send exactly one of 'value' or 'generate'".into(),
            )))
        }
    };
    let existed: bool = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM secrets WHERE app_id = $1 AND name = $2 AND project_id IS NOT DISTINCT FROM $3)",
        )
        .bind(app.app_id)
        .bind(&name)
        .bind(project_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;

    let handle = upsert_secret(
        &state.db,
        &state.config.auth.secret_key,
        app.org_id,
        Some(app.app_id),
        project_id,
        &name,
        "user",
        &value,
    )
    .await?;
    let services_updated: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM secret_bindings WHERE secret_id = $1")
            .bind(handle.id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;

    let scope = match project_id {
        Some(p) => Scope::project(app.org_id, p),
        None => Scope::org(app.org_id),
    };
    events::emit(
        &state.db,
        scope,
        if existed { "secret.updated" } else { "secret.created" },
        serde_json::json!({ "ref": handle.secret_ref, "name": name, "app": app.app_key, "environment": body.environment }),
    )
    .await;
    caller
        .audit(
            &state.db,
            "set_secret",
            "secret",
            Some(handle.id),
            serde_json::json!({ "name": name, "org_id": app.org_id }),
        )
        .await;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "ref": handle.secret_ref,
        "name": name,
        "environment": body.environment,
        // Bound services get the new value on their next deploy or restart.
        "bound_env_vars_updated": services_updated,
    }))))
}

#[derive(Deserialize)]
struct EnvQuery {
    environment: Option<String>,
}

async fn delete_secret(
    caller: IntegrationCaller,
    Path((org, app, name)): Path<(String, String, String)>,
    Query(q): Query<EnvQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app).await?;
    let project_id = match &q.environment {
        Some(env) => Some(env_project(&state.db, &app, env).await?),
        None => None,
    };
    let in_use: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM secret_bindings b JOIN secrets s ON s.id = b.secret_id
         WHERE s.app_id = $1 AND s.name = $2 AND s.project_id IS NOT DISTINCT FROM $3",
    )
    .bind(app.app_id)
    .bind(&name)
    .bind(project_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;
    if in_use > 0 {
        return Err(ApiAppError(AppError::Conflict(format!(
            "Secret '{name}' is used by {in_use} env var(s); remove them first"
        ))));
    }
    let deleted: Option<String> = sqlx::query_scalar(
        "DELETE FROM secrets WHERE app_id = $1 AND name = $2 AND project_id IS NOT DISTINCT FROM $3 RETURNING ref",
    )
    .bind(app.app_id)
    .bind(&name)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    let Some(secret_ref) = deleted else {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Secret '{name}' not found"
        ))));
    };
    events::emit(
        &state.db,
        Scope::org(app.org_id),
        "secret.deleted",
        serde_json::json!({ "ref": secret_ref, "name": name, "app": app.app_key, "environment": q.environment }),
    )
    .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "deleted": secret_ref }),
    )))
}

/// POST /secrets/:ref/reveal — the only way to read a value. Needs the
/// `secrets:reveal` scope; every call is audit-logged.
async fn reveal_secret(
    caller: IntegrationCaller,
    Path(secret_ref): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_REVEAL)?;
    let row: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "SELECT s.id, s.org_id, s.name, s.value_encrypted FROM secrets s WHERE s.ref = $1",
    )
    .bind(&secret_ref)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    // A secret in an org the caller can't reach looks the same as no secret.
    let Some((id, org_id, name, enc)) = row else {
        return Err(ApiAppError(AppError::NotFound("Secret not found".into())));
    };
    if !super::can_access_org(&state.db, &caller, org_id).await? {
        return Err(ApiAppError(AppError::NotFound("Secret not found".into())));
    }
    let value = crypto::decrypt(&state.config.auth.secret_key, &enc).map_err(ApiAppError)?;
    sqlx::query("UPDATE secrets SET last_revealed_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    caller
        .audit(
            &state.db,
            "reveal_secret",
            "secret",
            Some(id),
            serde_json::json!({ "name": name, "org_id": org_id }),
        )
        .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "ref": secret_ref, "name": name, "value": value }),
    )))
}

/// Env var / secret names: `A-Z 0-9 _`, starting with a letter or `_`.
pub fn validate_env_name(name: &str) -> Result<(), ApiAppError> {
    let ok = !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && !name.as_bytes()[0].is_ascii_digit();
    if ok {
        Ok(())
    } else {
        Err(ApiAppError(AppError::BadRequest(format!(
            "'{name}' must be an env var name: A-Z, 0-9 and '_', not starting with a digit"
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_values_have_expected_shape() {
        assert_eq!(generate(Generate::Password).len(), 32);
        assert_eq!(generate(Generate::Hex32).len(), 64);
        assert_eq!(generate(Generate::Base64_32).len(), 43);
        assert!(Uuid::parse_str(&generate(Generate::Uuid)).is_ok());
        assert_ne!(generate(Generate::Password), generate(Generate::Password));
    }

    #[test]
    fn env_names() {
        assert!(validate_env_name("DATABASE_URL").is_ok());
        assert!(validate_env_name("_X1").is_ok());
        assert!(validate_env_name("1X").is_err());
        assert!(validate_env_name("lower").is_err());
        assert!(validate_env_name("A-B").is_err());
    }
}
