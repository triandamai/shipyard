use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::request::Parts,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{error::OpenApiError, OpenApiState};

// ─── Scope constants ──────────────────────────────────────────────────────────

pub const SCOPE_READ:   &str = "read";
pub const SCOPE_DEPLOY: &str = "deploy";
pub const SCOPE_WRITE:  &str = "write";
pub const SCOPE_ADMIN:  &str = "admin";

// ─── Caller identity ─────────────────────────────────────────────────────────

/// Extracted from a valid `ship_*` API key on every request.
#[derive(Debug, Clone)]
pub struct ApiKeyUser {
    pub key_id:  Uuid,
    pub org_id:  Uuid,
    pub scopes:  Vec<String>,
    pub key_name: String,
}

impl ApiKeyUser {
    /// Returns `Ok(())` when the caller has the requested scope,
    /// or `Err(Forbidden)` otherwise.
    pub fn require_scope(&self, scope: &str) -> Result<(), OpenApiError> {
        if self.scopes.iter().any(|s| s == scope) {
            Ok(())
        } else {
            Err(OpenApiError::Forbidden(format!(
                "This API key does not have the '{}' scope",
                scope
            )))
        }
    }

    /// Returns `true` if the key has at least one of the given scopes.
    pub fn has_any_scope(&self, scopes: &[&str]) -> bool {
        scopes.iter().any(|s| self.scopes.iter().any(|k| k == s))
    }
}

// ─── Extractor ────────────────────────────────────────────────────────────────

#[async_trait]
impl FromRequestParts<OpenApiState> for ApiKeyUser {
    type Rejection = OpenApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &OpenApiState,
    ) -> Result<Self, Self::Rejection> {
        let State(st): State<OpenApiState> =
            State::from_request_parts(parts, state).await.map_err(|_| {
                OpenApiError::Internal("Failed to extract state".into())
            })?;

        let raw_key = extract_key(&parts.headers).ok_or_else(|| {
            OpenApiError::Unauthorized(
                "Missing API key. Provide it via 'Authorization: Bearer ship_...' \
                 or the 'X-API-Key' header."
                    .into(),
            )
        })?;

        if !raw_key.starts_with("ship_") {
            return Err(OpenApiError::Unauthorized(
                "Invalid API key format. Keys must start with 'ship_'.".into(),
            ));
        }

        let hash = hex::encode(Sha256::digest(raw_key.as_bytes()));

        let row = sqlx::query_as::<_, (Uuid, Uuid, String, Vec<String>)>(
            "SELECT id, org_id, name, scopes
             FROM api_keys
             WHERE key_hash = $1
               AND (expires_at IS NULL OR expires_at > NOW())",
        )
        .bind(&hash)
        .fetch_optional(&st.db)
        .await
        .map_err(|e| OpenApiError::Database(e.to_string()))?
        .ok_or_else(|| OpenApiError::Unauthorized("Invalid or expired API key.".into()))?;

        let (key_id, org_id, key_name, scopes) = row;

        // Fire-and-forget last_used_at update — don't fail the request on error
        let db = st.db.clone();
        tokio::spawn(async move {
            let _ = sqlx::query(
                "UPDATE api_keys SET last_used_at = NOW() WHERE id = $1",
            )
            .bind(key_id)
            .execute(&db)
            .await;
        });

        Ok(ApiKeyUser { key_id, org_id, scopes, key_name })
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn extract_key(headers: &axum::http::HeaderMap) -> Option<String> {
    // Prefer Authorization: Bearer <key>
    if let Some(auth) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            if let Some(key) = s.strip_prefix("Bearer ") {
                return Some(key.trim().to_string());
            }
        }
    }
    // Fallback: X-API-Key: <key>
    if let Some(v) = headers.get("x-api-key") {
        if let Ok(s) = v.to_str() {
            return Some(s.trim().to_string());
        }
    }
    None
}

// ─── Key generation ───────────────────────────────────────────────────────────

/// Generate a new API key and return `(full_key, prefix, hash)`.
///
/// - `full_key` — returned once to the caller, never stored.
/// - `prefix`   — first 8 hex chars after `ship_`, stored for display.
/// - `hash`     — SHA-256(full_key) hex, stored in the DB.
pub fn generate_key() -> (String, String, String) {
    // Two UUIDs give 244 bits of randomness — sufficient for a random token.
    let body = format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let full_key = format!("ship_{}", body);
    let prefix = body[..8].to_string();
    let hash = hex::encode(Sha256::digest(full_key.as_bytes()));
    (full_key, prefix, hash)
}
