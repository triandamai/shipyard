use axum::{
    extract::{ConnectInfo, State},
    http::{HeaderMap, HeaderValue, StatusCode, header::SET_COOKIE},
    routing::{get, post, put},
    Json, Router,
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::models::User;
use shipyard_db::redis as session;

use crate::error::ApiAppError;
use crate::AppState;
use super::{AuthUser, create_access_token, create_refresh_token, decode_token};

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginUserInfo {
    pub id: Uuid,
    pub email: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub user: LoginUserInfo,
}

#[derive(Debug, Deserialize, Default)]
pub struct RefreshRequest {
    /// Legacy field — still accepted for clients that haven't updated.
    /// Prefer the HttpOnly cookie set on login.
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct LogoutRequest {
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/me/password", put(change_password))
}

// ─── Cookie helpers ───────────────────────────────────────────────────────────

fn set_refresh_cookie(token: &str, max_age: u64) -> HeaderValue {
    let value = format!(
        "shipyard_refresh={token}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={max_age}"
    );
    HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn clear_refresh_cookie() -> HeaderValue {
    HeaderValue::from_static(
        "shipyard_refresh=; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age=0",
    )
}

/// Extract the `shipyard_refresh` value from the `Cookie` request header.
fn refresh_token_from_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            s.split(';').find_map(|part| {
                part.trim()
                    .strip_prefix("shipyard_refresh=")
                    .map(str::to_owned)
            })
        })
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// POST /auth/register
async fn register(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<ApiResponse<RegisterResponse>>), ApiAppError> {
    // Tight rate limit for registration
    if state.auth_limiter.check_key(&addr.ip()).is_err() {
        return Err(ApiAppError(AppError::RateLimit(
            "Too many registration attempts. Please wait a minute.".to_string(),
        )));
    }

    if body.email.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("Email is required".to_string())));
    }
    if body.password.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("Password is required".to_string())));
    }
    if body.password.len() < 8 {
        return Err(ApiAppError(AppError::Validation(
            "Password must be at least 8 characters".to_string(),
        )));
    }

    let existing: Option<User> = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, created_at, updated_at FROM users WHERE email = $1",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if existing.is_some() {
        return Err(ApiAppError(AppError::Conflict(format!(
            "Email '{}' is already registered",
            body.email
        ))));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(body.password.as_bytes(), &salt)
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to hash password: {e}"))))?
        .to_string();

    let user: User = sqlx::query_as::<_, User>(
        "INSERT INTO users (id, email, password_hash, created_at, updated_at)
         VALUES ($1, $2, $3, NOW(), NOW())
         RETURNING id, email, password_hash, created_at, updated_at",
    )
    .bind(Uuid::now_v7())
    .bind(&body.email)
    .bind(&password_hash)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(RegisterResponse {
            id: user.id,
            email: user.email,
            created_at: user.created_at,
        })),
    ))
}

/// POST /auth/login
///
/// Returns the access token in the response body and sets the refresh token
/// as an `HttpOnly; Secure; SameSite=Strict` cookie so it is never readable
/// by client-side JavaScript.
async fn login(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<ApiResponse<LoginResponse>>), ApiAppError> {
    // Tight rate limit — 10 req/min per IP
    if state.auth_limiter.check_key(&addr.ip()).is_err() {
        return Err(ApiAppError(AppError::RateLimit(
            "Too many login attempts. Please wait a minute.".to_string(),
        )));
    }

    let user: User = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, created_at, updated_at FROM users WHERE email = $1",
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::Unauthorized("Invalid email or password".to_string())))?;

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to parse password hash: {e}"))))?;

    Argon2::default()
        .verify_password(body.password.as_bytes(), &parsed_hash)
        .map_err(|_| ApiAppError(AppError::Unauthorized("Invalid email or password".to_string())))?;

    let access_token = create_access_token(
        user.id,
        &user.email,
        &state.config.auth.jwt_secret,
        state.config.auth.access_token_expiry,
    )
    .map_err(ApiAppError)?;

    let (refresh_token, jti) = create_refresh_token(
        user.id,
        &user.email,
        &state.config.auth.jwt_secret,
        state.config.auth.refresh_token_expiry,
    )
    .map_err(ApiAppError)?;

    session::save_session(
        &state.redis,
        &jti,
        &session::SessionData {
            user_id: user.id,
            email: user.email.clone(),
            created_at: Utc::now(),
        },
        state.config.auth.refresh_token_expiry,
    ).await;

    crate::middleware::audit::write_audit_log(
        &state.db,
        Some(user.id),
        "login",
        Some("user"),
        Some(user.id),
        None,
        None,
    ).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        set_refresh_cookie(&refresh_token, state.config.auth.refresh_token_expiry),
    );

    Ok((
        headers,
        Json(ApiResponse::ok(LoginResponse {
            access_token,
            user: LoginUserInfo {
                id: user.id,
                email: user.email,
            },
        })),
    ))
}

/// POST /auth/refresh
///
/// Reads the refresh token from the `shipyard_refresh` HttpOnly cookie.
/// Falls back to `refresh_token` in the request body for backwards compatibility.
async fn refresh(
    State(state): State<AppState>,
    req_headers: HeaderMap,
    body: Option<Json<RefreshRequest>>,
) -> Result<Json<ApiResponse<RefreshResponse>>, ApiAppError> {
    // Prefer HttpOnly cookie; fall back to explicit body field.
    let refresh_token = match refresh_token_from_cookie(&req_headers) {
        Some(t) => t,
        None => match body.and_then(|Json(b)| b.refresh_token) {
            Some(t) => t,
            None => {
                return Err(ApiAppError(AppError::Unauthorized(
                    "No refresh token provided".to_string(),
                )))
            }
        },
    };

    let claims = decode_token(&refresh_token, &state.config.auth.jwt_secret)
        .map_err(ApiAppError)?;

    if claims.token_type != "refresh" {
        return Err(ApiAppError(AppError::Unauthorized(
            "Token is not a refresh token".to_string(),
        )));
    }

    let user_id = Uuid::parse_str(&claims.sub)
        .map_err(|_| ApiAppError(AppError::Unauthorized("Invalid user ID in token".to_string())))?;

    if let Some(jti) = &claims.jti {
        if state.redis.is_some() {
            if session::get_session(&state.redis, jti).await.is_none() {
                return Err(ApiAppError(AppError::Unauthorized(
                    "Session expired or revoked. Please log in again.".to_string(),
                )));
            }
            session::touch_session(
                &state.redis,
                jti,
                state.config.auth.refresh_token_expiry,
            ).await;
        }
    }

    let access_token = create_access_token(
        user_id,
        &claims.email,
        &state.config.auth.jwt_secret,
        state.config.auth.access_token_expiry,
    )
    .map_err(ApiAppError)?;

    Ok(Json(ApiResponse::ok(RefreshResponse { access_token })))
}

/// POST /auth/logout
///
/// Revokes the Redis session and clears the refresh token cookie.
async fn logout(
    _auth_user: AuthUser,
    State(state): State<AppState>,
    req_headers: HeaderMap,
    body: Option<Json<LogoutRequest>>,
) -> Result<(HeaderMap, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    // Try cookie first, then body
    let refresh_token = refresh_token_from_cookie(&req_headers)
        .or_else(|| body.and_then(|Json(b)| b.refresh_token));

    if let Some(rt) = refresh_token {
        if let Ok(claims) = decode_token(&rt, &state.config.auth.jwt_secret) {
            if let Some(jti) = claims.jti {
                session::delete_session(&state.redis, &jti).await;
            }
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(SET_COOKIE, clear_refresh_cookie());

    Ok((
        headers,
        Json(ApiResponse::ok(serde_json::json!({
            "message": "Logged out successfully"
        }))),
    ))
}

/// GET /auth/me
async fn me(
    auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<MeResponse>>, ApiAppError> {
    let user: User = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, created_at, updated_at FROM users WHERE id = $1",
    )
    .bind(auth_user.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("User not found".to_string())))?;

    Ok(Json(ApiResponse::ok(MeResponse {
        id: user.id,
        email: user.email,
        created_at: user.created_at,
    })))
}

/// PUT /auth/me/password
async fn change_password(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    if body.new_password.len() < 8 {
        return Err(ApiAppError(AppError::Validation(
            "New password must be at least 8 characters".to_string(),
        )));
    }

    let user: User = sqlx::query_as::<_, User>(
        "SELECT id, email, password_hash, created_at, updated_at FROM users WHERE id = $1",
    )
    .bind(auth_user.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("User not found".to_string())))?;

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to parse hash: {e}"))))?;

    Argon2::default()
        .verify_password(body.current_password.as_bytes(), &parsed_hash)
        .map_err(|_| ApiAppError(AppError::Unauthorized(
            "Current password is incorrect".to_string(),
        )))?;

    let salt = SaltString::generate(&mut OsRng);
    let new_hash = Argon2::default()
        .hash_password(body.new_password.as_bytes(), &salt)
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to hash password: {e}"))))?
        .to_string();

    sqlx::query("UPDATE users SET password_hash = $1, updated_at = NOW() WHERE id = $2")
        .bind(&new_hash)
        .bind(auth_user.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Password updated successfully"
    }))))
}
