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
use crate::middleware::rbac;
use crate::AppState;

// ─── Engine ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbEngine {
    Postgres,
    Mysql,
    Mariadb,
    Redis,
    Mongodb,
}

impl DbEngine {
    pub fn default_port(&self) -> u16 {
        match self {
            DbEngine::Postgres => 5432,
            DbEngine::Mysql | DbEngine::Mariadb => 3306,
            DbEngine::Redis => 6379,
            DbEngine::Mongodb => 27017,
        }
    }
}

fn detect_engine(image: &str) -> Option<DbEngine> {
    let img = image.to_lowercase();
    // Ordered: more-specific patterns first
    if img.contains("mariadb") {
        Some(DbEngine::Mariadb)
    } else if img.contains("mysql") {
        Some(DbEngine::Mysql)
    } else if img.contains("postgres") || img.contains("postgis") {
        Some(DbEngine::Postgres)
    } else if img.contains("redis") {
        Some(DbEngine::Redis)
    } else if img.contains("mongo") {
        Some(DbEngine::Mongodb)
    } else {
        None
    }
}

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DbMetaResponse {
    pub engine:   Option<DbEngine>,
    pub host:     Option<String>,
    pub port:     Option<u16>,
    pub username: Option<String>,
    pub detected: bool,
}

/// Unified query request.
///
/// Field semantics differ by engine:
/// - Postgres / MySQL / MariaDB: standard SQL in `sql`; `database`, `username`, `password` required.
/// - Redis: `sql` = Redis command (e.g. `KEYS *`); `database` = DB index (0–15, default "0");
///   `username` optional (Redis 6+ ACL); `password` optional for unsecured servers.
/// - MongoDB: `sql` = JSON `{"collection":"users","filter":{},"sort":{},"limit":100,"skip":0}`;
///   `database` required; `username`/`password` optional.
#[derive(Debug, Deserialize)]
pub struct DbQueryRequest {
    pub engine:   DbEngine,
    pub host:     String,
    pub port:     u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub sql:      String,
}

#[derive(Debug, Serialize)]
pub struct DbQueryResponse {
    pub columns:          Vec<String>,
    pub rows:             Vec<Vec<serde_json::Value>>,
    pub row_count:        usize,
    pub truncated:        bool,
    pub execution_time_ms: u128,
}

const ROW_LIMIT: usize = 1000;

// ─── RBAC ────────────────────────────────────────────────────────────────────

async fn require_db_client_permission(
    db: &sqlx::PgPool,
    user_id: Uuid,
    service_id: Uuid,
) -> Result<(), ApiAppError> {
    rbac::require_service_permission(db, user_id, service_id, "service:write")
        .await
        .map_err(ApiAppError)
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/services/:service_id/db/meta", get(get_db_meta))
        .route("/services/:service_id/db/query", post(run_db_query))
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn get_db_meta(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<DbMetaResponse>>, ApiAppError> {
    require_db_client_permission(&state.db, auth_user.user_id, service_id).await?;

    let row = sqlx::query_as::<_, (String,)>(
        "SELECT image FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Service not found".to_string())))?;

    let (image,) = row;
    let engine = detect_engine(&image);

    // Fetch all env var keys for this service (values are encrypted; we only need keys to detect).
    // For username detection we need the plaintext value — but since these are service-owned env vars
    // the user set themselves, we store the unencrypted value in value_encrypted for non-secret vars.
    let env_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT key, value_encrypted FROM service_envs WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let username = engine.as_ref().and_then(|e| detect_username(e, &env_rows));

    // Always use the Docker-internal DNS hostname — the backend runs inside Docker
    // on the same overlay network so this resolves reliably in production.
    // Pattern: {label_prefix}-{service_id}  e.g. platform-55961cb1-d6ef-440f-84c0-962901e0a68e
    let host = format!("{}-{}", state.config.docker.label_prefix, service_id);
    let port = engine.as_ref().map(|e| e.default_port());
    let detected = engine.is_some();

    Ok(Json(ApiResponse::ok(DbMetaResponse {
        engine,
        host: Some(host),
        port,
        username,
        detected,
    })))
}

fn detect_username(engine: &DbEngine, envs: &[(String, String)]) -> Option<String> {
    let candidates: &[&str] = match engine {
        DbEngine::Postgres  => &["POSTGRES_USER", "PGUSER"],
        DbEngine::Mysql     => &["MYSQL_USER", "MYSQL_ROOT_USER"],
        DbEngine::Mariadb   => &["MARIADB_USER", "MYSQL_USER"],
        DbEngine::Redis     => &["REDIS_USER"],
        DbEngine::Mongodb   => &["MONGO_INITDB_ROOT_USERNAME", "MONGODB_ROOT_USERNAME", "MONGODB_USERNAME"],
    };
    for key in candidates {
        if let Some((_, val)) = envs.iter().find(|(k, _)| k == key) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

async fn run_db_query(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<DbQueryRequest>,
) -> Result<Json<ApiResponse<DbQueryResponse>>, ApiAppError> {
    require_db_client_permission(&state.db, auth_user.user_id, service_id).await?;

    if body.sql.trim().is_empty() {
        return Err(ApiAppError(AppError::BadRequest("Query cannot be empty".to_string())));
    }

    let result = match body.engine {
        DbEngine::Postgres => run_postgres_query(&body).await,
        DbEngine::Mysql | DbEngine::Mariadb => run_mysql_query(&body).await,
        DbEngine::Redis => run_redis_command(&body).await,
        DbEngine::Mongodb => run_mongodb_query(&body).await,
    };

    result
        .map(|r| Json(ApiResponse::ok(r)))
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))
}

// ─── PostgreSQL ───────────────────────────────────────────────────────────────

async fn run_postgres_query(
    req: &DbQueryRequest,
) -> Result<DbQueryResponse, Box<dyn std::error::Error + Send + Sync>> {
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
        .map_err(|_| "Connection timed out — check that the host and port are reachable")??;

    let start = Instant::now();
    let rows = sqlx::query(&req.sql).fetch_all(&mut conn).await?;
    let elapsed = start.elapsed().as_millis();

    let columns: Vec<String> = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();

    let truncated = rows.len() > ROW_LIMIT;
    let take = rows.len().min(ROW_LIMIT);

    let result_rows = rows[..take]
        .iter()
        .map(|row| {
            row.columns()
                .iter()
                .enumerate()
                .map(|(i, col)| pg_value_to_json(row, i, col.type_info().name()))
                .collect()
        })
        .collect();

    Ok(DbQueryResponse { columns, rows: result_rows, row_count: take, truncated, execution_time_ms: elapsed })
}

fn pg_value_to_json(row: &sqlx::postgres::PgRow, idx: usize, type_name: &str) -> serde_json::Value {
    use sqlx::Row;
    match type_name {
        "INT2" | "INT4" => row.try_get::<i32, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "INT8" | "OID"  => row.try_get::<i64, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "FLOAT4"        => row.try_get::<f32, _>(idx).ok().map(|v| serde_json::Value::from(v as f64)).unwrap_or(serde_json::Value::Null),
        "FLOAT8"        => row.try_get::<f64, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "BOOL"          => row.try_get::<bool, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "JSON" | "JSONB" => row.try_get::<serde_json::Value, _>(idx).unwrap_or(serde_json::Value::Null),
        _               => row.try_get::<String, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
    }
}

// ─── MySQL / MariaDB ──────────────────────────────────────────────────────────

async fn run_mysql_query(
    req: &DbQueryRequest,
) -> Result<DbQueryResponse, Box<dyn std::error::Error + Send + Sync>> {
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
        .map_err(|_| "Connection timed out — check that the host and port are reachable")??;

    let start = Instant::now();
    let rows = sqlx::query(&req.sql).fetch_all(&mut conn).await?;
    let elapsed = start.elapsed().as_millis();

    let columns: Vec<String> = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();

    let truncated = rows.len() > ROW_LIMIT;
    let take = rows.len().min(ROW_LIMIT);

    let result_rows = rows[..take]
        .iter()
        .map(|row| {
            row.columns()
                .iter()
                .enumerate()
                .map(|(i, col)| mysql_value_to_json(row, i, col.type_info().name()))
                .collect()
        })
        .collect();

    Ok(DbQueryResponse { columns, rows: result_rows, row_count: take, truncated, execution_time_ms: elapsed })
}

fn mysql_value_to_json(row: &sqlx::mysql::MySqlRow, idx: usize, type_name: &str) -> serde_json::Value {
    use sqlx::Row;
    let t = type_name.to_uppercase();
    if t.contains("BIGINT") {
        row.try_get::<i64, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null)
    } else if t.contains("INT") {
        row.try_get::<i32, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null)
    } else if t.contains("DOUBLE") || t.contains("DECIMAL") || t.contains("NUMERIC") {
        row.try_get::<f64, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null)
    } else if t.contains("FLOAT") {
        row.try_get::<f32, _>(idx).ok().map(|v| serde_json::Value::from(v as f64)).unwrap_or(serde_json::Value::Null)
    } else if t.contains("JSON") {
        row.try_get::<serde_json::Value, _>(idx).unwrap_or(serde_json::Value::Null)
    } else {
        row.try_get::<String, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null)
    }
}

// ─── Redis ────────────────────────────────────────────────────────────────────

async fn run_redis_command(
    req: &DbQueryRequest,
) -> Result<DbQueryResponse, Box<dyn std::error::Error + Send + Sync>> {
    use std::time::Duration;
    use tokio::time::timeout;

    let db_num: u8 = req.database.trim().parse().unwrap_or(0);

    let url = match (req.username.trim(), req.password.as_str()) {
        (u, p) if !u.is_empty() && !p.is_empty() =>
            format!("redis://{}:{}@{}:{}/{}", u, p, req.host, req.port, db_num),
        ("", p) if !p.is_empty() =>
            format!("redis://:{}@{}:{}/{}", p, req.host, req.port, db_num),
        _ =>
            format!("redis://{}:{}/{}", req.host, req.port, db_num),
    };

    let client = redis::Client::open(url.as_str())
        .map_err(|e| format!("Redis URL error: {e}"))?;

    let mut conn = timeout(Duration::from_secs(10), client.get_multiplexed_async_connection())
        .await
        .map_err(|_| "Connection timed out — check that the host and port are reachable")??;

    let parts = tokenize_command(req.sql.trim());
    if parts.is_empty() {
        return Err("Empty Redis command".into());
    }

    let mut cmd = redis::Cmd::new();
    for part in &parts {
        cmd.arg(part.as_bytes());
    }

    let start = Instant::now();
    let value: redis::Value = cmd.query_async(&mut conn)
        .await
        .map_err(|e| format!("Command error: {e}"))?;
    let elapsed = start.elapsed().as_millis();

    let (columns, mut rows) = redis_value_to_table(value);
    let truncated = rows.len() > ROW_LIMIT;
    rows.truncate(ROW_LIMIT);
    let row_count = rows.len();

    Ok(DbQueryResponse { columns, rows, row_count, truncated, execution_time_ms: elapsed })
}

/// Minimal shell-like tokenizer so `SET "my key" "hello world"` works.
fn tokenize_command(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut qchar = '"';

    for ch in s.chars() {
        match (in_quote, ch) {
            (false, '"') | (false, '\'') => { in_quote = true; qchar = ch; }
            (true, c) if c == qchar      => { in_quote = false; }
            (false, ' ') | (false, '\t') | (false, '\n') => {
                if !cur.is_empty() { parts.push(std::mem::take(&mut cur)); }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() { parts.push(cur); }
    parts
}

fn redis_value_to_table(value: redis::Value) -> (Vec<String>, Vec<Vec<serde_json::Value>>) {
    match value {
        redis::Value::Nil =>
            (vec!["result".into()], vec![vec![serde_json::Value::Null]]),
        redis::Value::Int(n) =>
            (vec!["result".into()], vec![vec![n.into()]]),
        redis::Value::BulkString(bytes) => {
            let s = String::from_utf8_lossy(&bytes).into_owned();
            (vec!["result".into()], vec![vec![s.into()]])
        }
        redis::Value::SimpleString(s) =>
            (vec!["result".into()], vec![vec![s.into()]]),
        redis::Value::Okay =>
            (vec!["result".into()], vec![vec!["OK".into()]]),
        redis::Value::Array(items) => {
            // HGETALL returns alternating field/value pairs — detect and unpack.
            let is_pairs = items.len() % 2 == 0
                && items.len() >= 2
                && items.iter().enumerate().all(|(i, v)| {
                    i % 2 != 0 || matches!(v, redis::Value::BulkString(_) | redis::Value::SimpleString(_))
                });

            if is_pairs {
                let rows = items
                    .chunks(2)
                    .map(|pair| vec![redis_scalar_to_json(pair[0].clone()), redis_scalar_to_json(pair[1].clone())])
                    .collect();
                (vec!["field".into(), "value".into()], rows)
            } else {
                let rows = items.into_iter().map(|v| vec![redis_scalar_to_json(v)]).collect();
                (vec!["value".into()], rows)
            }
        }
        redis::Value::Map(pairs) => {
            let rows = pairs
                .into_iter()
                .map(|(k, v)| vec![redis_scalar_to_json(k), redis_scalar_to_json(v)])
                .collect();
            (vec!["field".into(), "value".into()], rows)
        }
        _ => (vec!["result".into()], vec![vec!["(unsupported response type)".into()]]),
    }
}

fn redis_scalar_to_json(value: redis::Value) -> serde_json::Value {
    match value {
        redis::Value::Nil               => serde_json::Value::Null,
        redis::Value::Int(n)            => n.into(),
        redis::Value::BulkString(b)     => String::from_utf8_lossy(&b).into_owned().into(),
        redis::Value::SimpleString(s)   => s.into(),
        redis::Value::Okay              => "OK".into(),
        redis::Value::Array(items)      => serde_json::Value::Array(items.into_iter().map(redis_scalar_to_json).collect()),
        redis::Value::Double(f)         => f.into(),
        redis::Value::Boolean(b)        => b.into(),
        _ => "(complex)".into(),
    }
}

// ─── MongoDB ──────────────────────────────────────────────────────────────────

/// MongoDB query format (passed as JSON in the `sql` field):
/// ```json
/// {"collection":"users","filter":{},"sort":{"_id":-1},"limit":100,"skip":0}
/// ```
/// For a connection test, use `{"$ping":true}`.
async fn run_mongodb_query(
    req: &DbQueryRequest,
) -> Result<DbQueryResponse, Box<dyn std::error::Error + Send + Sync>> {
    use mongodb::{
        bson::{doc, Document},
        options::ClientOptions,
        Client,
    };
    use futures::TryStreamExt;
    use std::time::Duration;
    use tokio::time::timeout;

    // Build connection URI
    let uri = match (req.username.trim(), req.password.as_str()) {
        (u, p) if !u.is_empty() && !p.is_empty() =>
            format!("mongodb://{}:{}@{}:{}", u, p, req.host, req.port),
        _ =>
            format!("mongodb://{}:{}", req.host, req.port),
    };

    let opts = timeout(Duration::from_secs(10), ClientOptions::parse(&uri))
        .await
        .map_err(|_| "DNS/parse timed out")??;

    let client = Client::with_options(opts)?;
    let db = client.database(if req.database.trim().is_empty() { "admin" } else { req.database.trim() });

    // Parse the query JSON from the `sql` field
    let query: serde_json::Value = serde_json::from_str(req.sql.trim())
        .map_err(|e| format!("Invalid query JSON: {e}"))?;

    // Connection test / ping
    if query.get("$ping").is_some() {
        let start = Instant::now();
        timeout(Duration::from_secs(10), db.run_command(doc! { "ping": 1 }))
            .await
            .map_err(|_| "Ping timed out")??;
        let elapsed = start.elapsed().as_millis();
        return Ok(DbQueryResponse {
            columns: vec!["result".into()],
            rows: vec![vec!["pong".into()]],
            row_count: 1,
            truncated: false,
            execution_time_ms: elapsed,
        });
    }

    // List collections for the schema browser
    if query.get("$listCollections").is_some() {
        let start = Instant::now();
        let names = timeout(Duration::from_secs(10), db.list_collection_names())
            .await
            .map_err(|_| "List collections timed out")??;
        let elapsed = start.elapsed().as_millis();
        let row_count = names.len();
        let rows = names.into_iter()
            .map(|n| vec![serde_json::Value::String(n)])
            .collect();
        return Ok(DbQueryResponse {
            columns: vec!["collection".into()],
            rows,
            row_count,
            truncated: false,
            execution_time_ms: elapsed,
        });
    }

    let collection_name = query["collection"]
        .as_str()
        .ok_or("Query must include a \"collection\" field")?;

    let filter: Document = query
        .get("filter")
        .and_then(|f| mongodb::bson::from_bson::<Document>(to_bson(f)?).ok())
        .unwrap_or_default();

    let sort: Option<Document> = query
        .get("sort")
        .and_then(|s| mongodb::bson::from_bson::<Document>(to_bson(s)?).ok());

    let limit = query["limit"].as_i64().unwrap_or(100).clamp(1, ROW_LIMIT as i64);
    let skip  = query["skip"].as_i64().unwrap_or(0).max(0) as u64;

    let collection: mongodb::Collection<Document> = db.collection(collection_name);

    let mut find = collection.find(filter);
    find = find.limit(limit).skip(skip);
    if let Some(s) = sort { find = find.sort(s); }

    let start = Instant::now();
    let cursor = timeout(Duration::from_secs(10), find)
        .await
        .map_err(|_| "Query timed out")??;

    let docs: Vec<Document> = cursor.try_collect().await?;
    let elapsed = start.elapsed().as_millis();

    if docs.is_empty() {
        return Ok(DbQueryResponse {
            columns: vec![],
            rows: vec![],
            row_count: 0,
            truncated: false,
            execution_time_ms: elapsed,
        });
    }

    // Collect all unique field names in order of first appearance
    let mut columns: Vec<String> = Vec::new();
    for doc in &docs {
        for key in doc.keys() {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
    }

    let rows: Vec<Vec<serde_json::Value>> = docs
        .iter()
        .map(|doc| {
            columns
                .iter()
                .map(|col| bson_to_json(doc.get(col).cloned()))
                .collect()
        })
        .collect();

    let row_count = rows.len();
    Ok(DbQueryResponse { columns, rows, row_count, truncated: false, execution_time_ms: elapsed })
}

fn to_bson(v: &serde_json::Value) -> Option<mongodb::bson::Bson> {
    mongodb::bson::to_bson(v).ok()
}

fn bson_to_json(val: Option<mongodb::bson::Bson>) -> serde_json::Value {
    match val {
        None => serde_json::Value::Null,
        Some(b) => serde_json::to_value(&b).unwrap_or(serde_json::Value::Null),
    }
}
