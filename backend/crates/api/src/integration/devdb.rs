//! Development databases on a shared Postgres server.
//!
//! Each app gets one database and two login roles:
//!
//! - `<db>_app`: owns the database. Its connection string is the
//!   `DEV_DATABASE_URL` secret, injected into the dev container (never shown
//!   to the agent: the agent sees only the secret's ref).
//! - `<db>_owner`: the user's own login, `DEV_DATABASE_OWNER_URL`. It is a
//!   member of the app role, so it can read and change everything the app
//!   (and its migrations) create.
//!
//! `CONNECT` is revoked from `PUBLIC`, so no other tenant's role can open the
//! database. Each role has a connection limit to protect the shared server.

use axum::{
    extract::{Path, State},
    routing::put,
    Json, Router,
};
use serde::Serialize;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool};
use tokio::sync::OnceCell;
use uuid::Uuid;

use shipyard_common::config::IntegrationConfig;
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events::{self, Scope};

use super::secrets::{secret_value, upsert_secret};
use super::{app_ref, db_err, random_alnum, AppRef, IntegrationCaller, SCOPE_MANAGE};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/orgs/:org/apps/:app/dev-database",
        put(ensure_route).get(get_route).delete(delete_route),
    )
}

pub const APP_URL_SECRET: &str = "DEV_DATABASE_URL";
pub const OWNER_URL_SECRET: &str = "DEV_DATABASE_OWNER_URL";
const APP_CONNECTION_LIMIT: i32 = 20;
const OWNER_CONNECTION_LIMIT: i32 = 5;

static ADMIN_POOL: OnceCell<PgPool> = OnceCell::const_new();

async fn admin_pool(cfg: &IntegrationConfig) -> Result<&'static PgPool, ApiAppError> {
    let url = cfg.dev_db_admin_url.as_deref().ok_or_else(|| {
        ApiAppError(AppError::BadRequest(
            "Dev databases are not enabled on this Shipyard (integration.dev_db_admin_url is unset)".into(),
        ))
    })?;
    ADMIN_POOL
        .get_or_try_init(|| async {
            PgPoolOptions::new()
                .max_connections(4)
                .connect(url)
                .await
                .map_err(|e| {
                    ApiAppError(AppError::Internal(format!(
                        "Can't reach the dev database server: {e}"
                    )))
                })
        })
        .await
}

/// Host and port written into dev connection strings.
fn public_endpoint(cfg: &IntegrationConfig) -> Result<(String, u16), ApiAppError> {
    let admin: PgConnectOptions = cfg
        .dev_db_admin_url
        .as_deref()
        .unwrap_or_default()
        .parse()
        .map_err(|e| ApiAppError(AppError::Internal(format!("Invalid dev_db_admin_url: {e}"))))?;
    let host = cfg
        .dev_db_public_host
        .clone()
        .unwrap_or_else(|| admin.get_host().to_string());
    let port = cfg.dev_db_public_port.unwrap_or_else(|| admin.get_port());
    Ok((host, port))
}

pub fn connection_url(
    user: &str,
    password: &str,
    host: &str,
    port: u16,
    db: &str,
    sslmode: &str,
) -> String {
    // user, password and db are generated from [a-z0-9_] only, so no escaping.
    format!("postgres://{user}:{password}@{host}:{port}/{db}?sslmode={sslmode}")
}

/// Database and role names for an app. 36 chars + suffix, under Postgres's
/// 63-byte identifier limit.
pub fn names_for(app_id: Uuid) -> (String, String, String) {
    let db = format!("dev_{}", app_id.simple());
    (db.clone(), format!("{db}_app"), format!("{db}_owner"))
}

/// SQL to create one app's database and roles, in order. Identifiers and
/// passwords are generated (`[a-z0-9_]`), checked by `assert_safe`.
pub fn provision_statements(
    db: &str,
    app_role: &str,
    app_pw: &str,
    owner_role: &str,
    owner_pw: &str,
) -> Vec<String> {
    for v in [db, app_role, app_pw, owner_role, owner_pw] {
        assert_safe(v);
    }
    vec![
        format!("CREATE ROLE \"{app_role}\" LOGIN PASSWORD '{app_pw}' CONNECTION LIMIT {APP_CONNECTION_LIMIT}"),
        format!("CREATE ROLE \"{owner_role}\" LOGIN PASSWORD '{owner_pw}' CONNECTION LIMIT {OWNER_CONNECTION_LIMIT}"),
        format!("GRANT \"{app_role}\" TO \"{owner_role}\""),
        // A non-superuser admin must be able to SET ROLE to the new owner.
        format!("GRANT \"{app_role}\" TO CURRENT_USER"),
        format!("CREATE DATABASE \"{db}\" OWNER \"{app_role}\""),
        format!("REVOKE ALL ON DATABASE \"{db}\" FROM PUBLIC"),
    ]
}

pub fn drop_statements(db: &str, app_role: &str, owner_role: &str) -> Vec<String> {
    for v in [db, app_role, owner_role] {
        assert_safe(v);
    }
    vec![
        format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"),
        format!("DROP ROLE IF EXISTS \"{owner_role}\""),
        format!("DROP ROLE IF EXISTS \"{app_role}\""),
    ]
}

fn assert_safe(v: &str) {
    assert!(
        !v.is_empty()
            && v.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
        "unsafe identifier"
    );
}

#[derive(Debug, Serialize)]
pub struct DevDatabaseInfo {
    pub database: String,
    pub host: String,
    pub port: u16,
    pub app_role: String,
    pub owner_role: String,
    /// Ref of the secret holding the app's connection string. Inject it into
    /// the dev container's `DATABASE_URL`; never give the value to a model.
    pub app_url_ref: String,
    /// Ref of the user's own connection string (for their DB client).
    pub owner_url_ref: String,
    pub created: bool,
}

/// Create the app's dev database if it doesn't exist. Idempotent.
pub async fn ensure_dev_database(
    state: &AppState,
    app: &AppRef,
) -> Result<DevDatabaseInfo, ApiAppError> {
    let cfg = &state.config.integration;
    let (host, port) = public_endpoint(cfg)?;

    if let Some(info) = load(state, app, &host, port).await? {
        return Ok(info);
    }

    let pool = admin_pool(cfg).await?;
    let (db, app_role, owner_role) = names_for(app.app_id);
    let (app_pw, owner_pw) = (random_alnum(32), random_alnum(32));

    // Clear leftovers of an earlier half-finished attempt.
    for stmt in drop_statements(&db, &app_role, &owner_role) {
        sqlx::query(&stmt)
            .execute(pool)
            .await
            .map_err(provision_err)?;
    }
    for stmt in provision_statements(&db, &app_role, &app_pw, &owner_role, &owner_pw) {
        sqlx::query(&stmt)
            .execute(pool)
            .await
            .map_err(provision_err)?;
    }

    let sk = &state.config.auth.secret_key;
    let app_secret = upsert_secret(
        &state.db,
        sk,
        app.org_id,
        Some(app.app_id),
        None,
        APP_URL_SECRET,
        "dev_db_app",
        &connection_url(&app_role, &app_pw, &host, port, &db, &cfg.dev_db_sslmode),
    )
    .await?;
    let owner_secret = upsert_secret(
        &state.db,
        sk,
        app.org_id,
        Some(app.app_id),
        None,
        OWNER_URL_SECRET,
        "dev_db_owner",
        &connection_url(
            &owner_role,
            &owner_pw,
            &host,
            port,
            &db,
            &cfg.dev_db_sslmode,
        ),
    )
    .await?;

    sqlx::query(
        "INSERT INTO dev_databases (id, org_id, app_id, db_name, app_role, owner_role, app_secret_id, owner_secret_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(Uuid::now_v7())
    .bind(app.org_id)
    .bind(app.app_id)
    .bind(&db)
    .bind(&app_role)
    .bind(&owner_role)
    .bind(app_secret.id)
    .bind(owner_secret.id)
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    events::emit(
        &state.db,
        Scope::org(app.org_id),
        "dev_database.created",
        serde_json::json!({
            "app": app.app_key,
            "database": db,
            "app_url_ref": app_secret.secret_ref,
            "owner_url_ref": owner_secret.secret_ref,
        }),
    )
    .await;

    Ok(DevDatabaseInfo {
        database: db,
        host,
        port,
        app_role,
        owner_role,
        app_url_ref: app_secret.secret_ref,
        owner_url_ref: owner_secret.secret_ref,
        created: true,
    })
}

async fn load(
    state: &AppState,
    app: &AppRef,
    host: &str,
    port: u16,
) -> Result<Option<DevDatabaseInfo>, ApiAppError> {
    #[allow(clippy::type_complexity)]
    let row: Option<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT d.db_name, d.app_role, d.owner_role, sa.ref, so.ref
         FROM dev_databases d
         LEFT JOIN secrets sa ON sa.id = d.app_secret_id
         LEFT JOIN secrets so ON so.id = d.owner_secret_id
         WHERE d.app_id = $1",
    )
    .bind(app.app_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    Ok(row.map(
        |(database, app_role, owner_role, app_ref, owner_ref)| DevDatabaseInfo {
            database,
            host: host.to_string(),
            port,
            app_role,
            owner_role,
            app_url_ref: app_ref.unwrap_or_default(),
            owner_url_ref: owner_ref.unwrap_or_default(),
            created: false,
        },
    ))
}

/// Drop the app's dev database and roles. Missing is fine.
pub async fn drop_dev_database(state: &AppState, app: &AppRef) -> Result<bool, ApiAppError> {
    let row: Option<(String, String, String)> =
        sqlx::query_as("SELECT db_name, app_role, owner_role FROM dev_databases WHERE app_id = $1")
            .bind(app.app_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err)?;
    let Some((db, app_role, owner_role)) = row else {
        return Ok(false);
    };
    let pool = admin_pool(&state.config.integration).await?;
    for stmt in drop_statements(&db, &app_role, &owner_role) {
        sqlx::query(&stmt)
            .execute(pool)
            .await
            .map_err(provision_err)?;
    }
    sqlx::query("DELETE FROM secrets WHERE app_id = $1 AND kind IN ('dev_db_app', 'dev_db_owner')")
        .bind(app.app_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    sqlx::query("DELETE FROM dev_databases WHERE app_id = $1")
        .bind(app.app_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    events::emit(
        &state.db,
        Scope::org(app.org_id),
        "dev_database.deleted",
        serde_json::json!({ "app": app.app_key, "database": db }),
    )
    .await;
    Ok(true)
}

fn provision_err(e: sqlx::Error) -> ApiAppError {
    ApiAppError(AppError::Internal(format!("Dev database server: {e}")))
}

/// For tests and health checks: can the app role log in?
pub async fn check_login(state: &AppState, app: &AppRef) -> Result<(), ApiAppError> {
    let secret_id: Uuid =
        sqlx::query_scalar("SELECT app_secret_id FROM dev_databases WHERE app_id = $1")
            .bind(app.app_id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;
    let url = secret_value(&state.db, &state.config.auth.secret_key, secret_id).await?;
    let opts: PgConnectOptions = url
        .parse()
        .map_err(|e: sqlx::Error| ApiAppError(AppError::Internal(e.to_string())))?;
    let mut conn = opts.connect().await.map_err(provision_err)?;
    sqlx::query("SELECT 1")
        .execute(&mut conn)
        .await
        .map_err(provision_err)?;
    Ok(())
}

// ─── Routes ──────────────────────────────────────────────────────────────────

/// PUT /orgs/:org/apps/:app/dev-database — create if missing; returns refs,
/// never connection strings.
async fn ensure_route(
    caller: IntegrationCaller,
    Path((org, app)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<DevDatabaseInfo>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app).await?;
    let info = ensure_dev_database(&state, &app).await?;
    if info.created {
        caller
            .audit(
                &state.db,
                "create_dev_database",
                "dev_database",
                None,
                serde_json::json!({ "app": app.app_key, "org_id": app.org_id }),
            )
            .await;
    }
    Ok(Json(ApiResponse::ok(info)))
}

async fn get_route(
    caller: IntegrationCaller,
    Path((org, app)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<DevDatabaseInfo>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app).await?;
    let (host, port) = public_endpoint(&state.config.integration)?;
    load(&state, &app, &host, port)
        .await?
        .map(|i| Json(ApiResponse::ok(i)))
        .ok_or_else(|| ApiAppError(AppError::NotFound("This app has no dev database".into())))
}

async fn delete_route(
    caller: IntegrationCaller,
    Path((org, app)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app).await?;
    if !drop_dev_database(&state, &app).await? {
        return Err(ApiAppError(AppError::NotFound(
            "This app has no dev database".into(),
        )));
    }
    caller
        .audit(
            &state.db,
            "delete_dev_database",
            "dev_database",
            None,
            serde_json::json!({ "app": app.app_key, "org_id": app.org_id }),
        )
        .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "deleted": true }),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_fit_postgres_identifiers() {
        let (db, app, owner) = names_for(Uuid::now_v7());
        assert!(db.starts_with("dev_") && db.len() == 36);
        assert!(owner.len() <= 63 && app.len() <= 63);
    }

    #[test]
    #[should_panic(expected = "unsafe identifier")]
    fn rejects_unsafe_identifiers() {
        provision_statements("x\"; drop", "a", "b", "c", "d");
    }

    /// Runs the real statements against a Postgres server:
    /// `DEVDB_TEST_ADMIN_URL=postgres://admin:pw@127.0.0.1:5432/postgres cargo test -p shipyard-api devdb -- --ignored`
    /// (falls back to CI's `TEST_DATABASE_URL`; skipped when neither is set).
    /// The admin needs only CREATEDB and CREATEROLE, as in production.
    #[tokio::test]
    #[ignore]
    async fn provisioning_isolates_apps() {
        let Some(admin_url) = std::env::var("DEVDB_TEST_ADMIN_URL")
            .or_else(|_| std::env::var("TEST_DATABASE_URL"))
            .ok()
        else {
            eprintln!("skipped: set DEVDB_TEST_ADMIN_URL or TEST_DATABASE_URL");
            return;
        };
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&admin_url)
            .await
            .unwrap();
        let base: PgConnectOptions = admin_url.parse().unwrap();
        let (host, port) = (base.get_host().to_string(), base.get_port());

        let mut apps = vec![];
        for _ in 0..2 {
            let (db, app_role, owner_role) = names_for(Uuid::now_v7());
            let (app_pw, owner_pw) = (random_alnum(32), random_alnum(32));
            for stmt in provision_statements(&db, &app_role, &app_pw, &owner_role, &owner_pw) {
                sqlx::query(&stmt)
                    .execute(&admin)
                    .await
                    .unwrap_or_else(|e| panic!("{stmt}: {e}"));
            }
            apps.push((db, app_role, app_pw, owner_role, owner_pw));
        }
        let connect = |user: &str, pw: &str, db: &str| {
            let url = connection_url(user, pw, &host, port, db, "disable");
            async move { url.parse::<PgConnectOptions>().unwrap().connect().await }
        };

        let (db_a, app_a, app_pw_a, owner_a, owner_pw_a) = &apps[0];
        let (db_b, app_b, app_pw_b, _, _) = &apps[1];

        // The app role can create tables (migrations) in its own database.
        let mut c = connect(app_a, app_pw_a, db_a)
            .await
            .expect("app connects to its db");
        sqlx::query("CREATE TABLE todo (id serial primary key, title text)")
            .execute(&mut c)
            .await
            .unwrap();
        sqlx::query("INSERT INTO todo (title) VALUES ('x')")
            .execute(&mut c)
            .await
            .unwrap();

        // The user's owner role sees and changes what the app created.
        let mut o = connect(owner_a, owner_pw_a, db_a)
            .await
            .expect("owner connects");
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM todo")
            .fetch_one(&mut o)
            .await
            .unwrap();
        assert_eq!(n, 1);
        sqlx::query("ALTER TABLE todo ADD COLUMN done bool")
            .execute(&mut o)
            .await
            .expect("owner can alter app tables");

        // Another app's role can't open this database, and vice versa.
        assert!(
            connect(app_b, app_pw_b, db_a).await.is_err(),
            "app B must not reach app A's db"
        );
        assert!(
            connect(app_a, app_pw_a, db_b).await.is_err(),
            "app A must not reach app B's db"
        );
        // Wrong password is refused.
        assert!(connect(app_a, "wrong", db_a).await.is_err());

        drop(c);
        drop(o);
        for (db, app_role, _, owner_role, _) in &apps {
            for stmt in drop_statements(db, app_role, owner_role) {
                sqlx::query(&stmt)
                    .execute(&admin)
                    .await
                    .unwrap_or_else(|e| panic!("{stmt}: {e}"));
            }
        }
        let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pg_database WHERE datname = $1")
            .bind(db_a)
            .fetch_one(&admin)
            .await
            .unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn connection_url_shape() {
        assert_eq!(
            connection_url("u_app", "pw", "devdb.example", 5432, "dev_x", "require"),
            "postgres://u_app:pw@devdb.example:5432/dev_x?sslmode=require"
        );
    }
}
