pub mod routes;
pub mod oauth;

use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{request::Parts, HeaderMap},
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use uuid::Uuid;

use shipyard_common::error::{AppError, AppResult};

use crate::error::ApiAppError;
use crate::AppState;

// ─── JWT Claims ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,             // user UUID as string
    pub email: String,
    pub exp: usize,              // unix timestamp
    pub iat: usize,
    pub token_type: String,      // "access" | "refresh"
    pub jti: Option<String>,     // JWT ID — set on refresh tokens for session tracking
}

// ─── JWT Helpers ─────────────────────────────────────────────────────────────

pub fn create_access_token(
    user_id: Uuid,
    email: &str,
    secret: &str,
    expiry_secs: u64,
) -> AppResult<String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .as_secs();

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        iat: now as usize,
        exp: (now + expiry_secs) as usize,
        token_type: "access".to_string(),
        jti: None,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("Failed to create access token: {e}")))
}

/// Create a refresh token. Returns `(token_string, jti)` so the caller can
/// store the jti in Redis without needing to re-decode the token.
pub fn create_refresh_token(
    user_id: Uuid,
    email: &str,
    secret: &str,
    expiry_secs: u64,
) -> AppResult<(String, String)> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::Internal(e.to_string()))?
        .as_secs();

    let jti = Uuid::now_v7().to_string();

    let claims = Claims {
        sub: user_id.to_string(),
        email: email.to_string(),
        iat: now as usize,
        exp: (now + expiry_secs) as usize,
        token_type: "refresh".to_string(),
        jti: Some(jti.clone()),
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(format!("Failed to create refresh token: {e}")))?;

    Ok((token, jti))
}

pub fn decode_token(token: &str, secret: &str) -> AppResult<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| AppError::Unauthorized(format!("Invalid token: {e}")))
}

// ─── Auth Extractor ──────────────────────────────────────────────────────────

/// Authenticated user extracted from a valid "access" JWT.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub email: String,
}

fn extract_bearer_token(headers: &HeaderMap) -> AppResult<&str> {
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?
        .to_str()
        .map_err(|_| AppError::Unauthorized("Invalid Authorization header encoding".to_string()))?;

    auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Authorization header must start with 'Bearer '".to_string()))
}

#[async_trait]
impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiAppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let State(app_state): State<AppState> = State::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiAppError(AppError::Internal("Failed to extract app state".to_string())))?;

        let token = extract_bearer_token(&parts.headers).map_err(ApiAppError)?;

        // API key path: tokens prefixed with "ship_"
        if token.starts_with("ship_") {
            return resolve_api_key(token, &app_state).await;
        }

        // JWT path
        let claims = decode_token(token, &app_state.config.auth.jwt_secret).map_err(ApiAppError)?;

        if claims.token_type != "access" {
            return Err(ApiAppError(AppError::Unauthorized(
                "Token is not an access token".to_string(),
            )));
        }

        let user_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| ApiAppError(AppError::Unauthorized("Invalid user ID in token".to_string())))?;

        Ok(AuthUser {
            user_id,
            email: claims.email,
        })
    }
}

/// Validate a `ship_...` API key and return the owning user.
async fn resolve_api_key(key: &str, state: &AppState) -> Result<AuthUser, ApiAppError> {
    let hash = hex::encode(Sha256::digest(key.as_bytes()));

    let row = sqlx::query_as::<_, (Uuid, String)>(
        r#"SELECT u.id, u.email
           FROM api_keys ak
           JOIN users u ON u.id = ak.created_by
           WHERE ak.key_hash = $1
             AND (ak.expires_at IS NULL OR ak.expires_at > NOW())"#,
    )
    .bind(&hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::Unauthorized("Invalid or expired API key".to_string())))?;

    let (user_id, email) = row;

    // Touch last_used_at in the background — don't block the request.
    let db = state.db.clone();
    let hash_clone = hash.clone();
    tokio::spawn(async move {
        let _ = sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE key_hash = $1")
            .bind(&hash_clone)
            .execute(&db)
            .await;
    });

    Ok(AuthUser { user_id, email })
}
