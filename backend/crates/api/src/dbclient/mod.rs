use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, TypeInfo};
use std::time::Instant;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Engine detection ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbEngine {
    Postgres,
    Mysql,
    Mariadb,
}

impl DbEngine {
    pub fn default_port(&self) -> u16 {
        match self {
            DbEngine::Postgres => 5432,
            DbEngine::Mysql | DbEngine::Mariadb => 3306,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            DbEngine::Postgres => "PostgreSQL",
            DbEngine::Mysql => "MySQL",
            DbEngine::Mariadb => "MariaDB",
        }
    }
}

/// Detect the database engine from a Docker image name (lowercase match).
fn detect_engine(image: &str) -> Option<DbEngine> {
    let img = image.to_lowercase();
    // Check mariadb before mysql since "mariadb" contains no "mysql"
    if img.contains("mariadb") {
        Some(DbEngine::Mariadb)
    } else if img.contains("mysql") {
        Some(DbEngine::Mysql)
    } else if img.contains("postgres") || img.contains("postgis") {
        Some(DbEngine::Postgres)
    } else {
        None
    }
}

// ─── Request / Response types ─────────────────────────────────────────────────

/// Response for GET /services/:service_id/db/meta
#[derive(Debug, Serialize)]
pub struct DbMetaResponse {
    /// Detected engine, or null if we couldn't determine it from the image.
    pub engine: Option<DbEngine>,
    /// Internal Docker network IP of the running container, or null.
    pub host: Option<String>,
    /// Default port for the detected engine, or null.
    pub port: Option<u16>,
    /// True when engine/host/port were all successfully auto-detected.
    pub detected: bool,
}

/// Body for POST /services/:service_id/db/query
#[derive(Debug, Deserialize)]
pub struct DbQueryRequest {
    pub engine: DbEngine,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub sql: String,
}

/// Response for POST /services/:service_id/db/query
#[derive(Debug, Serialize)]
pub struct DbQueryResponse {
    pub columns: Vec<String>,
    /// Rows as arrays of JSON values, matching `columns` order.
    pub rows: Vec<Vec<serde_json::Value>>,
    /// Number of rows returned (capped at ROW_LIMIT).
    pub row_count: usize,
    /// True if results were truncated to ROW_LIMIT.
    pub truncated: bool,
    pub execution_time_ms: u128,
}

const ROW_LIMIT: usize = 1000;

// ─── RBAC helper ─────────────────────────────────────────────────────────────

/// Verify the caller is a member of the org that owns the service's project.
async fn require_service_access(
    db: &sqlx::PgPool,
    user_id: Uuid,
    service_id: Uuid,
) -> Result<(), ApiAppError> {
    let row = sqlx::query_as::<_, (Uuid,)>(
        "SELECT p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (org_id,) = row.ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id)))
    })?;

    let is_member = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if is_member.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "You are not a member of this organization.".to_string(),
        )));
    }
    Ok(())
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/services/:service_id/db/meta",
            get(get_db_meta),
        )
        .route(
            "/services/:service_id/db/query",
            post(run_db_query),
        )
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// GET /services/:service_id/db/meta
///
/// Returns auto-detected connection metadata using the same Docker DNS hostname
/// that the service overview panel uses — reachable from the backend via the
/// internal overlay network.
async fn get_db_meta(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<DbMetaResponse>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await?;

    let row = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT image, ports FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Service not found".to_string())))?;

    let (image, ports) = row;
    let engine = detect_engine(&image);

    // Extract the host-side published port from ["hostPort:containerPort", ...]
    // The backend runs on the same host as the Docker daemon so it can always
    // reach published ports via 127.0.0.1 — no Docker DNS needed.
    let host_port = ports
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .and_then(|s| s.split(':').next())   // "hostPort:containerPort" → hostPort
        .and_then(|s| s.parse::<u16>().ok())
        .filter(|&p| p > 0);

    let (host, port, detected) = if let Some(hp) = host_port {
        // Published port → connect via localhost (works from host process in dev and prod)
        (Some("127.0.0.1".to_string()), Some(hp), engine.is_some())
    } else {
        // No published port → use the Docker internal DNS hostname.
        // This resolves only when the backend itself runs inside Docker on the same
        // network (production). In dev (backend on host) it will not resolve.
        let docker_host = format!("{}-{}", state.config.docker.label_prefix, service_id);
        (Some(docker_host), engine.as_ref().map(|e| e.default_port()), engine.is_some())
    };

    Ok(Json(ApiResponse::ok(DbMetaResponse {
        engine,
        host,
        port,
        detected,
    })))
}

/// POST /services/:service_id/db/query
///
/// Opens a fresh connection using the provided credentials and executes one
/// SQL statement. Results are capped at ROW_LIMIT rows.
async fn run_db_query(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<DbQueryRequest>,
) -> Result<Json<ApiResponse<DbQueryResponse>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await?;

    if body.sql.trim().is_empty() {
        return Err(ApiAppError(AppError::BadRequest("SQL query cannot be empty".to_string())));
    }

    let result = match body.engine {
        DbEngine::Postgres => run_postgres_query(&body).await,
        DbEngine::Mysql | DbEngine::Mariadb => run_mysql_query(&body).await,
    };

    result
        .map(|r| Json(ApiResponse::ok(r)))
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))
}

// ─── PostgreSQL executor ──────────────────────────────────────────────────────

async fn run_postgres_query(req: &DbQueryRequest) -> Result<DbQueryResponse, Box<dyn std::error::Error + Send + Sync>> {
    use sqlx::postgres::PgConnectOptions;
    use sqlx::ConnectOptions;
    use std::time::Duration;
    use tokio::time::timeout;

    let opts = PgConnectOptions::new()
        .host(&req.host)
        .port(req.port)
        .database(&req.database)
        .username(&req.username)
        .password(&req.password);

    let mut conn = timeout(Duration::from_secs(10), opts.connect())
        .await
        .map_err(|_| "Connection timed out — check that the host and port are reachable")?
        ?;

    let start = Instant::now();
    let rows = sqlx::query(&req.sql)
        .fetch_all(&mut conn)
        .await?;
    let elapsed = start.elapsed().as_millis();

    let columns: Vec<String> = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();

    let truncated = rows.len() > ROW_LIMIT;
    let rows_to_return = if truncated { ROW_LIMIT } else { rows.len() };

    let result_rows: Vec<Vec<serde_json::Value>> = rows[..rows_to_return]
        .iter()
        .map(|row| {
            row.columns()
                .iter()
                .enumerate()
                .map(|(i, col)| pg_value_to_json(row, i, col.type_info().name()))
                .collect()
        })
        .collect();

    Ok(DbQueryResponse {
        columns,
        rows: result_rows,
        row_count: rows_to_return,
        truncated,
        execution_time_ms: elapsed,
    })
}

fn pg_value_to_json(row: &sqlx::postgres::PgRow, idx: usize, type_name: &str) -> serde_json::Value {
    use sqlx::Row;
    match type_name {
        "INT2" | "INT4" => row
            .try_get::<i32, _>(idx)
            .ok()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
        "INT8" | "OID" => row
            .try_get::<i64, _>(idx)
            .ok()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
        "FLOAT4" => row
            .try_get::<f32, _>(idx)
            .ok()
            .map(|v| serde_json::Value::from(v as f64))
            .unwrap_or(serde_json::Value::Null),
        "FLOAT8" => row
            .try_get::<f64, _>(idx)
            .ok()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
        "BOOL" => row
            .try_get::<bool, _>(idx)
            .ok()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
        "JSON" | "JSONB" => row
            .try_get::<serde_json::Value, _>(idx)
            .unwrap_or(serde_json::Value::Null),
        // UUID, TEXT, VARCHAR, CHAR, BPCHAR, TIMESTAMP, TIMESTAMPTZ, DATE, TIME, INET,
        // NUMERIC (returned as string to preserve precision), etc.
        _ => row
            .try_get::<String, _>(idx)
            .ok()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    }
}

// ─── MySQL / MariaDB executor ─────────────────────────────────────────────────

async fn run_mysql_query(req: &DbQueryRequest) -> Result<DbQueryResponse, Box<dyn std::error::Error + Send + Sync>> {
    use sqlx::mysql::MySqlConnectOptions;
    use sqlx::ConnectOptions;
    use std::time::Duration;
    use tokio::time::timeout;

    let opts = MySqlConnectOptions::new()
        .host(&req.host)
        .port(req.port)
        .database(&req.database)
        .username(&req.username)
        .password(&req.password);

    let mut conn = timeout(Duration::from_secs(10), opts.connect())
        .await
        .map_err(|_| "Connection timed out — check that the host and port are reachable")?
        ?;

    let start = Instant::now();
    let rows = sqlx::query(&req.sql)
        .fetch_all(&mut conn)
        .await?;
    let elapsed = start.elapsed().as_millis();

    let columns: Vec<String> = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();

    let truncated = rows.len() > ROW_LIMIT;
    let rows_to_return = if truncated { ROW_LIMIT } else { rows.len() };

    let result_rows: Vec<Vec<serde_json::Value>> = rows[..rows_to_return]
        .iter()
        .map(|row| {
            row.columns()
                .iter()
                .enumerate()
                .map(|(i, col)| mysql_value_to_json(row, i, col.type_info().name()))
                .collect()
        })
        .collect();

    Ok(DbQueryResponse {
        columns,
        rows: result_rows,
        row_count: rows_to_return,
        truncated,
        execution_time_ms: elapsed,
    })
}

fn mysql_value_to_json(row: &sqlx::mysql::MySqlRow, idx: usize, type_name: &str) -> serde_json::Value {
    use sqlx::Row;
    let upper = type_name.to_uppercase();
    match upper.as_str() {
        t if t.contains("TINYINT") || t.contains("SMALLINT") || t.contains("MEDIUMINT") || t.contains("INT") && !t.contains("BIGINT") => {
            row.try_get::<i32, _>(idx)
                .ok()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null)
        }
        t if t.contains("BIGINT") => {
            row.try_get::<i64, _>(idx)
                .ok()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null)
        }
        t if t.contains("FLOAT") => {
            row.try_get::<f32, _>(idx)
                .ok()
                .map(|v| serde_json::Value::from(v as f64))
                .unwrap_or(serde_json::Value::Null)
        }
        t if t.contains("DOUBLE") || t.contains("DECIMAL") || t.contains("NUMERIC") => {
            row.try_get::<f64, _>(idx)
                .ok()
                .map(serde_json::Value::from)
                .unwrap_or(serde_json::Value::Null)
        }
        t if t.contains("JSON") => {
            row.try_get::<serde_json::Value, _>(idx)
                .unwrap_or(serde_json::Value::Null)
        }
        _ => row
            .try_get::<String, _>(idx)
            .ok()
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null),
    }
}
