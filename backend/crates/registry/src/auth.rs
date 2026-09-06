use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine as _;
use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

use sha2::{Digest as _, Sha256};

use crate::{error::{RegistryError, Result}, RegistryState};

/// OCI token request query params.
/// GET /auth/registry/token?service=<hostname>&scope=artifact:<ns>/<repo>:pull|push
#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    pub service: Option<String>,
    pub scope:   Option<String>,
    pub account: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RegistryClaims {
    sub:    String,
    iss:    String,
    aud:    String,
    exp:    i64,
    iat:    i64,
    access: Vec<AccessEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccessEntry {
    #[serde(rename = "type")]
    kind:    String,
    name:    String,
    actions: Vec<String>,
}

/// Issue a short-lived registry JWT (15 min) when the caller presents a valid
/// Shipyard Bearer/Basic token via the Authorization header.
///
/// Two token kinds are accepted:
///   - Shipyard user JWT — any authenticated user; full push+pull access.
///   - Shipyard API key (`ship_…`) — access is gated by `registry:view` (pull)
///     or `registry:manage` (pull + push) scopes stored on the key.
pub async fn issue_token(
    State(state): State<RegistryState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
) -> Result<Response> {
    // Extract Bearer or Basic credential from Authorization header.
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or(RegistryError::Unauthorized)?;

    // Docker sends Basic auth for `docker login`: base64("username:api_token").
    // Bearer auth is used by the Docker CLI after a token has been issued.
    let token: String = if let Some(bearer) = auth_header.strip_prefix("Bearer ") {
        bearer.to_string()
    } else if let Some(encoded) = auth_header.strip_prefix("Basic ") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| RegistryError::Unauthorized)?;
        let s = String::from_utf8(decoded).map_err(|_| RegistryError::Unauthorized)?;
        // "username:password" — username is ignored; password is the Shipyard API token.
        s.splitn(2, ':')
            .nth(1)
            .ok_or(RegistryError::Unauthorized)?
            .to_string()
    } else {
        return Err(RegistryError::Unauthorized);
    };

    // Determine subject and allowed actions based on token kind.
    let (subject, allowed_actions) = if token.starts_with("ship_") {
        // API key path — look up by hash, check registry scopes.
        let hash = hex::encode(Sha256::digest(token.as_bytes()));
        let row: Option<(uuid::Uuid, uuid::Uuid, Vec<String>, Option<chrono::DateTime<Utc>>)> =
            sqlx::query_as(
                "SELECT id, org_id, scopes, expires_at FROM api_keys WHERE key_hash = $1",
            )
            .bind(&hash)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| RegistryError::Internal(e.to_string()))?;

        let (key_id, org_id, scopes, expires_at) = row.ok_or(RegistryError::Unauthorized)?;

        if let Some(exp) = expires_at {
            if exp < Utc::now() {
                return Err(RegistryError::Unauthorized);
            }
        }

        let org_str = org_id.to_string();
        let view_perm   = format!("shipyard:{org_str}:registry:view");
        let manage_perm = format!("shipyard:{org_str}:registry:manage");
        let has_manage = scopes.contains(&manage_perm);
        let has_view   = scopes.contains(&view_perm) || has_manage;

        if !has_view {
            return Err(RegistryError::Unauthorized);
        }

        let actions = if has_manage {
            vec!["pull".to_string(), "push".to_string()]
        } else {
            vec!["pull".to_string()]
        };

        // Update last_used_at without blocking the response.
        let db = state.db.clone();
        tokio::spawn(async move {
            let _ = sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
                .bind(key_id)
                .execute(&db)
                .await;
        });

        (key_id.to_string(), Some(actions))
    } else {
        // User JWT path — any authenticated user gets full access.
        let sub = verify_shipyard_token(&token, &state.jwt_secret)
            .ok_or(RegistryError::Unauthorized)?;
        (sub, None)
    };

    // Parse requested scope and intersect with allowed actions.
    let access = q.scope
        .as_deref()
        .map(|s| filter_scope(s, allowed_actions.as_deref()))
        .unwrap_or_default();

    let now = Utc::now().timestamp();
    let claims = RegistryClaims {
        sub:    subject,
        iss:    state.hostname.clone(),
        aud:    q.service.unwrap_or_else(|| state.hostname.clone()),
        iat:    now,
        exp:    now + 900, // 15 minutes
        access,
    };

    let jwt = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|e| RegistryError::Internal(e.to_string()))?;

    Ok(Json(serde_json::json!({ "token": jwt, "expires_in": 900 })).into_response())
}

/// Middleware helper: extract and verify the registry JWT from Authorization: Bearer <token>.
/// Returns the subject (user id) if valid.
pub fn verify_registry_token(token: &str, jwt_secret: &str) -> Option<String> {
    use jsonwebtoken::{decode, DecodingKey, Validation};
    let mut val = Validation::new(Algorithm::HS256);
    val.validate_aud = false;
    decode::<RegistryClaims>(token, &DecodingKey::from_secret(jwt_secret.as_bytes()), &val)
        .ok()
        .map(|d| d.claims.sub)
}

/// Parse "artifact:myorg/myproject/api:pull,push" and optionally intersect
/// the requested actions with `allowed` (pass `None` for unrestricted access).
fn filter_scope(scope: &str, allowed: Option<&[String]>) -> Vec<AccessEntry> {
    let parts: Vec<&str> = scope.splitn(3, ':').collect();
    if parts.len() != 3 {
        return vec![];
    }
    let requested: Vec<String> = parts[2].split(',').map(|s| s.to_string()).collect();
    let actions = match allowed {
        None => requested,
        Some(allowed) => requested.into_iter().filter(|a| allowed.contains(a)).collect(),
    };
    if actions.is_empty() {
        return vec![];
    }
    vec![AccessEntry {
        kind:    parts[0].to_string(),
        name:    parts[1].to_string(),
        actions,
    }]
}

/// Verify a Shipyard API JWT and return its `sub` claim.
fn verify_shipyard_token(token: &str, secret: &str) -> Option<String> {
    use jsonwebtoken::{decode, DecodingKey, Validation};
    #[derive(Deserialize)]
    struct Claims { sub: String }
    let mut val = Validation::new(Algorithm::HS256);
    val.validate_aud = false;
    decode::<Claims>(token, &DecodingKey::from_secret(secret.as_bytes()), &val)
        .ok()
        .map(|d| d.claims.sub)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};

    #[test]
    fn filter_scope_parses_type_name_actions() {
        let entries = filter_scope("artifact:myorg/myproject/api:pull,push", None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "artifact");
        assert_eq!(entries[0].name, "myorg/myproject/api");
        assert_eq!(entries[0].actions, vec!["pull", "push"]);
    }

    #[test]
    fn filter_scope_intersects_with_allowed_actions() {
        let allowed = vec!["pull".to_string()];
        let entries = filter_scope("artifact:myorg/api:pull,push", Some(&allowed));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actions, vec!["pull"]);
    }

    #[test]
    fn filter_scope_returns_empty_when_no_actions_allowed() {
        let allowed = vec!["push".to_string()];
        let entries = filter_scope("artifact:myorg/api:pull", Some(&allowed));
        assert!(entries.is_empty());
    }

    #[test]
    fn filter_scope_rejects_malformed_scope_strings() {
        assert!(filter_scope("not-enough-parts", None).is_empty());
        assert!(filter_scope("only:two", None).is_empty());
        assert!(filter_scope("", None).is_empty());
    }

    fn sign(claims: &serde_json::Value, secret: &str) -> String {
        encode(&Header::new(Algorithm::HS256), claims, &EncodingKey::from_secret(secret.as_bytes())).unwrap()
    }

    #[test]
    fn verify_shipyard_token_roundtrips_a_valid_token() {
        let now = Utc::now().timestamp();
        let claims = serde_json::json!({ "sub": "user-123", "exp": now + 3600, "iat": now });
        let token = sign(&claims, "test-secret");
        assert_eq!(verify_shipyard_token(&token, "test-secret"), Some("user-123".to_string()));
    }

    #[test]
    fn verify_shipyard_token_rejects_wrong_secret() {
        let now = Utc::now().timestamp();
        let claims = serde_json::json!({ "sub": "user-123", "exp": now + 3600, "iat": now });
        let token = sign(&claims, "test-secret");
        assert_eq!(verify_shipyard_token(&token, "wrong-secret"), None);
    }

    #[test]
    fn verify_shipyard_token_rejects_expired_token() {
        // Well past jsonwebtoken's default 60s leeway, so this is unambiguously expired.
        let now = Utc::now().timestamp();
        let claims = serde_json::json!({ "sub": "user-123", "exp": now - 3700, "iat": now - 7200 });
        let token = sign(&claims, "test-secret");
        assert_eq!(verify_shipyard_token(&token, "test-secret"), None);
    }

    #[test]
    fn verify_registry_token_roundtrips_full_claims() {
        let now = Utc::now().timestamp();
        let claims = RegistryClaims {
            sub: "svc-account".to_string(),
            iss: "registry.example.com".to_string(),
            aud: "registry.example.com".to_string(),
            exp: now + 900,
            iat: now,
            access: vec![],
        };
        let token = sign(&serde_json::to_value(&claims).unwrap(), "reg-secret");
        assert_eq!(verify_registry_token(&token, "reg-secret"), Some("svc-account".to_string()));
    }

    #[test]
    fn verify_registry_token_rejects_garbage_input() {
        assert_eq!(verify_registry_token("not-a-jwt", "any-secret"), None);
    }
}
