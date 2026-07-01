use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::models::SystemConfig;

use crate::error::ApiAppError;
use crate::AppState;

// ─── Constants ───────────────────────────────────────────────────────────────

const SETUP_STATUS_KEY: &str = "setup_status";
const STATUS_DONE: &str = "done";

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SetupStatusResponse {
    pub initialized: bool,
    pub step: String,
}

#[derive(Debug, Serialize)]
pub struct CheckDockerResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct CheckSwarmResponse {
    pub swarm_active: bool,
    pub node_id: Option<String>,
    pub node_addr: Option<String>,
    pub manager: bool,
    pub nodes: u64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct InitRequest {
    pub admin_email: String,
    pub admin_password: String,
    pub org_name: String,
    #[serde(default)]
    pub org_slug: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InitResponse {
    pub message: String,
    pub admin_user_id: Uuid,
    pub org_id: Uuid,
}

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/status", get(setup_status))
        .route("/check-docker", post(check_docker))
        .route("/check-swarm", post(check_swarm))
        .route("/init", post(init))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Returns the current setup status value from system_config, or "uninitialised" if not set.
async fn get_setup_status(db: &sqlx::PgPool) -> Result<String, ApiAppError> {
    let row: Option<SystemConfig> = sqlx::query_as::<_, SystemConfig>(
        "SELECT key, value, updated_at FROM system_config WHERE key = $1",
    )
    .bind(SETUP_STATUS_KEY)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(match row {
        Some(cfg) => cfg
            .value
            .as_str()
            .unwrap_or("uninitialised")
            .to_string(),
        None => "uninitialised".to_string(),
    })
}

/// Determines the wizard step from the raw status string.
fn status_to_step(status: &str) -> &'static str {
    match status {
        STATUS_DONE => "done",
        "admin_create" => "admin_create",
        "docker_check" => "docker_check",
        _ => "uninitialised",
    }
}

/// Returns true if the platform is fully initialized.
pub async fn is_initialized(db: &sqlx::PgPool) -> bool {
    match get_setup_status(db).await {
        Ok(s) => s == STATUS_DONE,
        Err(_) => false,
    }
}

// ─── Middleware ───────────────────────────────────────────────────────────────

/// Axum middleware that gates all routes except the setup + health paths.
/// If the platform is not yet initialized, returns 503 with NOT_INITIALIZED.
pub async fn require_initialized_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let path = req.uri().path().to_owned();

    // The middleware runs inside .nest("/api", ...) so Axum has already stripped
    // the "/api" prefix — paths here are "/setup/init", "/status", etc.
    let is_exempt = path.starts_with("/setup/")
        || path == "/status"
        || path.starts_with("/auth/oauth/");

    if is_exempt {
        return next.run(req).await;
    }

    // Check initialization
    if !is_initialized(&state.db).await {
        let body = Json(ApiResponse::<()>::err(
            "NOT_INITIALIZED",
            "Platform not initialized. Complete setup at /api/setup/init",
        ));
        return axum::response::IntoResponse::into_response((StatusCode::SERVICE_UNAVAILABLE, body));
    }

    next.run(req).await
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// GET /setup/status
async fn setup_status(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SetupStatusResponse>>, ApiAppError> {
    let status = get_setup_status(&state.db).await?;
    let step = status_to_step(&status);
    let initialized = status == STATUS_DONE;

    Ok(Json(ApiResponse::ok(SetupStatusResponse {
        initialized,
        step: step.to_string(),
    })))
}

/// POST /setup/check-docker
async fn check_docker(
    State(_state): State<AppState>,
) -> Result<Json<ApiResponse<CheckDockerResponse>>, ApiAppError> {
    // Attempt to connect to the Docker socket by checking if the socket file exists
    // and is accessible. A real check would open the socket, but for the wizard
    // we test for the socket path existence which is safe without the Docker SDK.
    let socket_paths = ["/var/run/docker.sock", "/run/docker.sock"];

    let accessible = socket_paths.iter().any(|p| std::path::Path::new(p).exists());

    if accessible {
        Ok(Json(ApiResponse::ok(CheckDockerResponse {
            success: true,
            message: "Docker socket is accessible".to_string(),
        })))
    } else {
        Ok(Json(ApiResponse::ok(CheckDockerResponse {
            success: false,
            message: "Docker socket not found. Ensure Docker is running and the socket is mounted".to_string(),
        })))
    }
}

/// POST /setup/check-swarm
async fn check_swarm(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CheckSwarmResponse>>, ApiAppError> {
    match state.docker.swarm_info().await {
        Ok(info) => Ok(Json(ApiResponse::ok(CheckSwarmResponse {
            swarm_active: true,
            node_id: Some(info.node_id),
            node_addr: Some(info.node_addr),
            manager: info.is_manager,
            nodes: info.nodes,
            message: "Node is part of an active Swarm cluster".to_string(),
        }))),
        Err(_) => Ok(Json(ApiResponse::ok(CheckSwarmResponse {
            swarm_active: false,
            node_id: None,
            node_addr: None,
            manager: false,
            nodes: 0,
            message: "Node is not part of a Swarm. Run `docker swarm init` first.".to_string(),
        }))),
    }
}

/// POST /setup/init
///
/// Creates the admin user, default organization, and org membership, then marks
/// the platform as initialized by writing "done" to system_config.
async fn init(
    State(state): State<AppState>,
    Json(body): Json<InitRequest>,
) -> Result<(StatusCode, Json<ApiResponse<InitResponse>>), ApiAppError> {
    // Guard: already initialized
    let current_status = get_setup_status(&state.db).await?;
    if current_status == STATUS_DONE {
        return Err(ApiAppError(AppError::Conflict(
            "Platform is already initialized".to_string(),
        )));
    }

    // Validate inputs
    if body.admin_email.is_empty() || !body.admin_email.contains('@') {
        return Err(ApiAppError(AppError::Validation(
            "A valid admin email is required".to_string(),
        )));
    }
    if body.admin_password.len() < 8 {
        return Err(ApiAppError(AppError::Validation(
            "Password must be at least 8 characters".to_string(),
        )));
    }
    if body.org_name.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation(
            "Organization name is required".to_string(),
        )));
    }

    // Ensure the email is not already taken
    let existing_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE email = $1")
            .bind(&body.admin_email)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if existing_count.0 > 0 {
        return Err(ApiAppError(AppError::Conflict(format!(
            "Email '{}' is already registered",
            body.admin_email
        ))));
    }

    // Hash password
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(body.admin_password.as_bytes(), &salt)
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to hash password: {e}"))))?
        .to_string();

    // Generate IDs up front
    let admin_user_id = Uuid::new_v4();
    let org_id = Uuid::new_v4();

    // Use the provided slug if non-empty; otherwise derive one from the org name.
    let org_slug: String = match body.org_slug.as_deref().filter(|s| !s.is_empty()) {
        Some(s) => s.to_string(),
        None => body
            .org_name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-"),
    };

    // Run everything inside a transaction so it's all-or-nothing
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 1. Insert admin user
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, created_at, updated_at)
         VALUES ($1, $2, $3, NOW(), NOW())",
    )
    .bind(admin_user_id)
    .bind(&body.admin_email)
    .bind(&password_hash)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(format!("Failed to create admin user: {e}"))))?;

    // 2. Insert organization
    sqlx::query(
        "INSERT INTO organizations (id, name, slug, created_at)
         VALUES ($1, $2, $3, NOW())",
    )
    .bind(org_id)
    .bind(&body.org_name)
    .bind(&org_slug)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(format!("Failed to create organization: {e}"))))?;

    // 3. Insert org membership (owner role)
    sqlx::query(
        "INSERT INTO org_members (id, org_id, user_id, role, created_at)
         VALUES ($1, $2, $3, 'owner'::member_role, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(admin_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(format!("Failed to create org member: {e}"))))?;

    // 4. Set setup_status = "done"
    sqlx::query(
        r#"
        INSERT INTO system_config (key, value, updated_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT (key) DO UPDATE
            SET value = EXCLUDED.value,
                updated_at = NOW()
        "#,
    )
    .bind(SETUP_STATUS_KEY)
    .bind(serde_json::Value::String(STATUS_DONE.to_string()))
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(format!("Failed to set setup status: {e}"))))?;

    tx.commit()
        .await
        .map_err(|e| ApiAppError(AppError::Database(format!("Transaction commit failed: {e}"))))?;

    tracing::info!(
        admin_user_id = %admin_user_id,
        org_id = %org_id,
        "Platform initialized successfully"
    );

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(InitResponse {
            message: "Platform initialized successfully".to_string(),
            admin_user_id,
            org_id,
        })),
    ))
}

