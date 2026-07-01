use chrono::{DateTime, Utc};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const PREFIX: &str = "shipyard:session:";

// ─── Session data ─────────────────────────────────────────────────────────────

/// Data persisted in Redis for an active user session.
/// Keyed by the refresh token's `jti` (JWT ID).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    pub user_id: Uuid,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

fn session_key(jti: &str) -> String {
    format!("{PREFIX}{jti}")
}

// ─── Session operations ───────────────────────────────────────────────────────

/// Persist a session keyed by the refresh token's jti.
/// No-op (not an error) when Redis is not configured.
pub async fn save_session(
    redis: &Option<ConnectionManager>,
    jti: &str,
    data: &SessionData,
    ttl_secs: u64,
) {
    let Some(mut conn) = redis.as_ref().map(|r| r.clone()) else { return };
    let Ok(json) = serde_json::to_string(data) else { return };
    let _ = redis::cmd("SET")
        .arg(session_key(jti))
        .arg(json)
        .arg("EX")
        .arg(ttl_secs)
        .query_async::<()>(&mut conn)
        .await;
}

/// Fetch a session by jti. Returns None when not found, expired, or Redis unavailable.
pub async fn get_session(
    redis: &Option<ConnectionManager>,
    jti: &str,
) -> Option<SessionData> {
    let mut conn = redis.as_ref()?.clone();
    let json: Option<String> = redis::cmd("GET")
        .arg(session_key(jti))
        .query_async(&mut conn)
        .await
        .ok()
        .flatten();
    json.as_deref().and_then(|s| serde_json::from_str(s).ok())
}

/// Refresh the TTL of an existing session (sliding window on token refresh).
pub async fn touch_session(
    redis: &Option<ConnectionManager>,
    jti: &str,
    ttl_secs: u64,
) {
    let Some(mut conn) = redis.as_ref().map(|r| r.clone()) else { return };
    let _ = redis::cmd("EXPIRE")
        .arg(session_key(jti))
        .arg(ttl_secs)
        .query_async::<()>(&mut conn)
        .await;
}

/// Delete a session (on logout or explicit revocation).
/// No-op when Redis is not configured.
pub async fn delete_session(redis: &Option<ConnectionManager>, jti: &str) {
    let Some(mut conn) = redis.as_ref().map(|r| r.clone()) else { return };
    let _ = redis::cmd("DEL")
        .arg(session_key(jti))
        .query_async::<()>(&mut conn)
        .await;
}
