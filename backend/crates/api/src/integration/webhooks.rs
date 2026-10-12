//! Outbound webhooks for platform events, and the events feed.
//!
//! Every request is signed: `Shipyard-Signature: t=<unix>,v1=<hex>` where
//! `v1 = HMAC-SHA256(secret, "<t>.<raw body>")`. Deliveries that don't get a
//! 2xx within 10 s are retried with backoff for about a day, then marked
//! failed (redeliverable). Delivery is at-least-once and unordered: dedupe on
//! the event `id` and order by `created_at` (ids are time-ordered UUIDv7).

use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use shipyard_common::crypto;
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events;

use super::{db_err, org_by_external, random_hex, IntegrationCaller, SCOPE_MANAGE};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/webhooks", get(list_endpoints).post(create_endpoint))
        .route(
            "/webhooks/:id",
            axum::routing::patch(update_endpoint).delete(delete_endpoint),
        )
        .route("/webhooks/:id/rotate-secret", post(rotate_secret))
        .route("/webhooks/:id/test", post(test_endpoint))
        .route("/webhooks/:id/deliveries", get(list_deliveries))
        .route("/deliveries/:id/redeliver", post(redeliver))
        .route("/orgs/:org/events", get(list_events))
        .route("/event-types", get(event_types))
}

/// Seconds to wait before attempt N+1 after N failed attempts.
const BACKOFF_SECS: [i64; 10] = [10, 30, 120, 600, 1800, 3600, 7200, 14400, 28800, 43200];

pub fn backoff_after(attempts: i32) -> Option<i64> {
    BACKOFF_SECS.get((attempts.max(1) - 1) as usize).copied()
}

pub fn sign(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!(
        "t={timestamp},v1={}",
        hex::encode(mac.finalize().into_bytes())
    )
}

// ─── Envelope ────────────────────────────────────────────────────────────────

/// The JSON body of every webhook and every item of the events feed.
pub async fn envelope(db: &sqlx::PgPool, event_id: Uuid) -> Option<serde_json::Value> {
    #[allow(clippy::type_complexity)]
    let row: Option<(Uuid, String, DateTime<Utc>, serde_json::Value, Uuid, Option<String>, Option<Uuid>, Option<Uuid>, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            "SELECT e.id, e.type, e.created_at, e.data, e.org_id, e.org_external_id, e.project_id, e.service_id,
                    COALESCE(a.app_key, e.data->>'app'), COALESCE(p.environment, e.data->>'environment'), s.slug
             FROM platform_events e
             LEFT JOIN projects p ON p.id = e.project_id
             LEFT JOIN integration_apps a ON a.id = p.app_id
             LEFT JOIN services s ON s.id = e.service_id
             WHERE e.id = $1",
        )
        .bind(event_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();
    let (
        id,
        event_type,
        created_at,
        data,
        org_id,
        external_id,
        project_id,
        service_id,
        app_key,
        environment,
        service,
    ) = row?;
    Some(serde_json::json!({
        "id": id,
        "type": event_type,
        "created_at": created_at,
        "org": { "id": org_id, "external_id": external_id },
        "app": app_key,
        "environment": environment,
        "project_id": project_id,
        "service_id": service_id,
        "service": service,
        "data": data,
    }))
}

// ─── Dispatcher ──────────────────────────────────────────────────────────────

/// Background loop: send due deliveries. Safe to run on several replicas
/// (rows are leased with SKIP LOCKED).
pub async fn run_dispatcher(state: std::sync::Arc<AppState>) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Shipyard-Webhooks/1")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("webhook dispatcher disabled: {e}");
            return;
        }
    };
    let mut last_prune = std::time::Instant::now() - Duration::from_secs(3600);
    loop {
        if last_prune.elapsed() > Duration::from_secs(3600) {
            last_prune = std::time::Instant::now();
            prune_old_events(&state.db).await;
        }
        match dispatch_due(&state, &client).await {
            Ok(0) => tokio::time::sleep(Duration::from_secs(2)).await,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("webhook dispatcher: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

/// Drop events older than 30 days once nothing is still trying to deliver them.
async fn prune_old_events(db: &sqlx::PgPool) {
    let result = sqlx::query(
        "DELETE FROM platform_events e
         WHERE e.created_at < NOW() - INTERVAL '30 days'
           AND NOT EXISTS (SELECT 1 FROM webhook_deliveries d WHERE d.event_id = e.id AND d.status = 'pending')",
    )
    .execute(db)
    .await;
    match result {
        Ok(r) if r.rows_affected() > 0 => {
            tracing::info!("pruned {} old platform events", r.rows_affected())
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("event pruning failed: {e}"),
    }
}

async fn dispatch_due(state: &AppState, client: &reqwest::Client) -> Result<usize, sqlx::Error> {
    // Lease up to 32 due deliveries for 2 minutes; a crash mid-send just
    // retries them after the lease.
    let due: Vec<(Uuid, Uuid, i32, String, String)> = sqlx::query_as(
        "UPDATE webhook_deliveries d
         SET next_attempt_at = NOW() + INTERVAL '2 minutes'
         FROM webhook_endpoints we
         WHERE d.id IN (
             SELECT id FROM webhook_deliveries
             WHERE status = 'pending' AND next_attempt_at <= NOW()
             ORDER BY next_attempt_at
             LIMIT 32
             FOR UPDATE SKIP LOCKED
         ) AND we.id = d.endpoint_id
         RETURNING d.id, d.event_id, d.attempts, we.url, we.secret_encrypted",
    )
    .fetch_all(&state.db)
    .await?;

    let n = due.len();
    let sends = due
        .into_iter()
        .map(|(delivery_id, event_id, attempts, url, secret_enc)| {
            let client = client.clone();
            async move {
                let outcome = match envelope(&state.db, event_id).await {
                    None => Err((None, "event no longer exists".to_string())),
                    Some(body) => {
                        let secret = crypto::decrypt_or_passthrough(
                            &state.config.auth.secret_key,
                            &secret_enc,
                        );
                        send(&client, &url, &secret, delivery_id, &body).await
                    }
                };
                record_attempt(&state.db, delivery_id, attempts + 1, outcome).await;
            }
        });
    futures::future::join_all(sends).await;
    Ok(n)
}

type SendOutcome = Result<u16, (Option<u16>, String)>;

async fn send(
    client: &reqwest::Client,
    url: &str,
    secret: &str,
    delivery_id: Uuid,
    body: &serde_json::Value,
) -> SendOutcome {
    let raw = serde_json::to_vec(body).map_err(|e| (None, e.to_string()))?;
    let ts = Utc::now().timestamp();
    let resp = client
        .post(url)
        .header("content-type", "application/json")
        .header("shipyard-event-id", body["id"].as_str().unwrap_or_default())
        .header(
            "shipyard-event-type",
            body["type"].as_str().unwrap_or_default(),
        )
        .header("shipyard-delivery-id", delivery_id.to_string())
        .header("shipyard-signature", sign(secret, ts, &raw))
        .body(raw)
        .send()
        .await
        .map_err(|e| (None, e.to_string()))?;
    let status = resp.status().as_u16();
    if resp.status().is_success() {
        Ok(status)
    } else {
        let text: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect();
        Err((Some(status), format!("HTTP {status}: {text}")))
    }
}

async fn record_attempt(db: &sqlx::PgPool, delivery_id: Uuid, attempts: i32, outcome: SendOutcome) {
    let result = match outcome {
        Ok(code) => {
            sqlx::query(
                "UPDATE webhook_deliveries
                 SET status = 'delivered', attempts = $2, last_status_code = $3, last_error = NULL, delivered_at = NOW()
                 WHERE id = $1",
            )
            .bind(delivery_id)
            .bind(attempts)
            .bind(code as i32)
            .execute(db)
            .await
        }
        Err((code, error)) => {
            let (status, wait) = match backoff_after(attempts) {
                Some(secs) => ("pending", secs),
                None => ("failed", 0),
            };
            sqlx::query(
                "UPDATE webhook_deliveries
                 SET status = $2, attempts = $3, last_status_code = $4, last_error = $5,
                     next_attempt_at = NOW() + make_interval(secs => $6)
                 WHERE id = $1",
            )
            .bind(delivery_id)
            .bind(status)
            .bind(attempts)
            .bind(code.map(|c| c as i32))
            .bind(error)
            .bind(wait as f64)
            .execute(db)
            .await
        }
    };
    if let Err(e) = result {
        tracing::warn!(%delivery_id, "failed to record webhook attempt: {e}");
    }
}

// ─── Endpoint management ─────────────────────────────────────────────────────

#[derive(Serialize)]
struct EndpointOut {
    id: Uuid,
    url: String,
    events: Vec<String>,
    enabled: bool,
    /// The org this endpoint is limited to (its external id), or null for
    /// every org of the integration.
    org: Option<String>,
    created_at: DateTime<Utc>,
}

async fn load_endpoint(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    id: Uuid,
) -> Result<EndpointOut, ApiAppError> {
    sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Vec<String>,
            bool,
            Option<String>,
            DateTime<Utc>,
        ),
    >(
        "SELECT we.id, we.url, we.events, we.enabled, o.external_id, we.created_at
         FROM webhook_endpoints we LEFT JOIN organizations o ON o.id = we.org_id
         WHERE we.id = $1 AND we.integration_id = $2",
    )
    .bind(id)
    .bind(caller.integration_id)
    .fetch_optional(db)
    .await
    .map_err(db_err)?
    .map(|(id, url, events, enabled, org, created_at)| EndpointOut {
        id,
        url,
        events,
        enabled,
        org,
        created_at,
    })
    .ok_or_else(|| ApiAppError(AppError::NotFound("Webhook endpoint not found".into())))
}

async fn list_endpoints(
    caller: IntegrationCaller,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<EndpointOut>>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            Vec<String>,
            bool,
            Option<String>,
            DateTime<Utc>,
        ),
    >(
        "SELECT we.id, we.url, we.events, we.enabled, o.external_id, we.created_at
         FROM webhook_endpoints we LEFT JOIN organizations o ON o.id = we.org_id
         WHERE we.integration_id = $1 ORDER BY we.created_at",
    )
    .bind(caller.integration_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(
        rows.into_iter()
            .map(|(id, url, events, enabled, org, created_at)| EndpointOut {
                id,
                url,
                events,
                enabled,
                org,
                created_at,
            })
            .collect(),
    )))
}

#[derive(Deserialize)]
struct CreateEndpoint {
    url: String,
    #[serde(default = "all_events")]
    events: Vec<String>,
    /// Limit to one org (its external id). Omit for every org.
    org: Option<String>,
}

fn all_events() -> Vec<String> {
    vec!["*".to_string()]
}

fn validate_url(url: &str) -> Result<(), ApiAppError> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("Invalid URL: {e}"))))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(ApiAppError(AppError::BadRequest(
            "URL must be http(s) with a host".into(),
        )));
    }
    Ok(())
}

fn validate_patterns(patterns: &[String]) -> Result<(), ApiAppError> {
    if patterns.is_empty() {
        return Err(ApiAppError(AppError::BadRequest(
            "events must not be empty".into(),
        )));
    }
    for p in patterns {
        if !events::pattern_is_known(p) {
            return Err(ApiAppError(AppError::BadRequest(format!(
                "'{p}' matches no event type. See GET /event-types."
            ))));
        }
    }
    Ok(())
}

/// POST /webhooks — the signing secret is in this response only.
async fn create_endpoint(
    caller: IntegrationCaller,
    State(state): State<AppState>,
    Json(body): Json<CreateEndpoint>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_url(&body.url)?;
    validate_patterns(&body.events)?;
    let org_id = match &body.org {
        Some(ext) => Some(org_by_external(&state.db, &caller, ext).await?),
        None => None,
    };
    let secret = format!("whsec_{}", random_hex(24));
    let enc = crypto::encrypt(&state.config.auth.secret_key, &secret).map_err(ApiAppError)?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO webhook_endpoints (id, integration_id, org_id, url, secret_encrypted, events)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(caller.integration_id)
    .bind(org_id)
    .bind(&body.url)
    .bind(&enc)
    .bind(&body.events)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;
    caller
        .audit(
            &state.db,
            "create_webhook",
            "webhook_endpoint",
            Some(id),
            serde_json::json!({ "url": body.url }),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(serde_json::json!({
            "id": id,
            "url": body.url,
            "events": body.events,
            "org": body.org,
            "secret": secret,
            "note": "Store the secret now; it is not shown again. Verify each request's Shipyard-Signature with it.",
        }))),
    ))
}

#[derive(Deserialize)]
struct UpdateEndpoint {
    url: Option<String>,
    events: Option<Vec<String>>,
    enabled: Option<bool>,
}

async fn update_endpoint(
    caller: IntegrationCaller,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<UpdateEndpoint>,
) -> Result<Json<ApiResponse<EndpointOut>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    load_endpoint(&state.db, &caller, id).await?;
    if let Some(url) = &body.url {
        validate_url(url)?;
    }
    if let Some(events) = &body.events {
        validate_patterns(events)?;
    }
    sqlx::query(
        "UPDATE webhook_endpoints
         SET url = COALESCE($2, url), events = COALESCE($3, events), enabled = COALESCE($4, enabled)
         WHERE id = $1",
    )
    .bind(id)
    .bind(&body.url)
    .bind(&body.events)
    .bind(body.enabled)
    .execute(&state.db)
    .await
    .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(
        load_endpoint(&state.db, &caller, id).await?,
    )))
}

async fn delete_endpoint(
    caller: IntegrationCaller,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    load_endpoint(&state.db, &caller, id).await?;
    sqlx::query("DELETE FROM webhook_endpoints WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    caller
        .audit(
            &state.db,
            "delete_webhook",
            "webhook_endpoint",
            Some(id),
            serde_json::json!({}),
        )
        .await;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": id }))))
}

async fn rotate_secret(
    caller: IntegrationCaller,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    load_endpoint(&state.db, &caller, id).await?;
    let secret = format!("whsec_{}", random_hex(24));
    let enc = crypto::encrypt(&state.config.auth.secret_key, &secret).map_err(ApiAppError)?;
    sqlx::query("UPDATE webhook_endpoints SET secret_encrypted = $2 WHERE id = $1")
        .bind(id)
        .bind(&enc)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    caller
        .audit(
            &state.db,
            "rotate_webhook_secret",
            "webhook_endpoint",
            Some(id),
            serde_json::json!({}),
        )
        .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "id": id, "secret": secret }),
    )))
}

/// POST /webhooks/:id/test — send a signed `ping` now and report the result.
async fn test_endpoint(
    caller: IntegrationCaller,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let ep = load_endpoint(&state.db, &caller, id).await?;
    let secret_enc: String =
        sqlx::query_scalar("SELECT secret_encrypted FROM webhook_endpoints WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;
    let secret = crypto::decrypt_or_passthrough(&state.config.auth.secret_key, &secret_enc);
    let body = serde_json::json!({
        "id": Uuid::now_v7(),
        "type": "ping",
        "created_at": Utc::now(),
        "org": null, "app": null, "environment": null,
        "project_id": null, "service_id": null, "service": null,
        "data": { "webhook_id": id },
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Shipyard-Webhooks/1")
        .build()
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    let result = send(&client, &ep.url, &secret, Uuid::now_v7(), &body).await;
    Ok(Json(ApiResponse::ok(match result {
        Ok(code) => serde_json::json!({ "ok": true, "status_code": code }),
        Err((code, error)) => {
            serde_json::json!({ "ok": false, "status_code": code, "error": error })
        }
    })))
}

#[derive(Deserialize)]
struct DeliveriesQuery {
    status: Option<String>,
    limit: Option<i64>,
}

async fn list_deliveries(
    caller: IntegrationCaller,
    Path(id): Path<Uuid>,
    Query(q): Query<DeliveriesQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    load_endpoint(&state.db, &caller, id).await?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(
        Uuid,
        Uuid,
        String,
        String,
        i32,
        Option<i32>,
        Option<String>,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        DateTime<Utc>,
    )> = sqlx::query_as(
        "SELECT d.id, d.event_id, e.type, d.status, d.attempts, d.last_status_code, d.last_error,
                    d.next_attempt_at, d.delivered_at, d.created_at
             FROM webhook_deliveries d JOIN platform_events e ON e.id = d.event_id
             WHERE d.endpoint_id = $1 AND ($2::text IS NULL OR d.status = $2)
             ORDER BY d.created_at DESC LIMIT $3",
    )
    .bind(id)
    .bind(&q.status)
    .bind(q.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(
        rows.into_iter()
            .map(
                |(
                    id,
                    event_id,
                    event_type,
                    status,
                    attempts,
                    code,
                    error,
                    next_at,
                    delivered_at,
                    created_at,
                )| {
                    serde_json::json!({
                        "id": id, "event_id": event_id, "event_type": event_type, "status": status,
                        "attempts": attempts, "last_status_code": code, "last_error": error,
                        "next_attempt_at": if status == "pending" { Some(next_at) } else { None },
                        "delivered_at": delivered_at, "created_at": created_at,
                    })
                },
            )
            .collect(),
    )))
}

async fn redeliver(
    caller: IntegrationCaller,
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let n = sqlx::query(
        "UPDATE webhook_deliveries d SET status = 'pending', attempts = 0, next_attempt_at = NOW()
         FROM webhook_endpoints we
         WHERE d.id = $1 AND we.id = d.endpoint_id AND we.integration_id = $2",
    )
    .bind(id)
    .bind(caller.integration_id)
    .execute(&state.db)
    .await
    .map_err(db_err)?
    .rows_affected();
    if n == 0 {
        return Err(ApiAppError(AppError::NotFound("Delivery not found".into())));
    }
    Ok(Json(ApiResponse::ok(serde_json::json!({ "queued": id }))))
}

// ─── Events feed ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct EventsQuery {
    /// Return events after this event id (exclusive). Poll with the last id you saw.
    after: Option<Uuid>,
    /// Exact type or prefix pattern (`deployment.*`).
    #[serde(rename = "type")]
    event_type: Option<String>,
    app: Option<String>,
    limit: Option<i64>,
}

/// GET /orgs/:org/events — the same envelopes webhooks carry, oldest first.
/// A pull alternative for when webhooks can't reach you, and for catching up.
async fn list_events(
    caller: IntegrationCaller,
    Path(org): Path<String>,
    Query(q): Query<EventsQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let org_id = org_by_external(&state.db, &caller, &org).await?;
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let ids: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT e.id, e.type FROM platform_events e
         LEFT JOIN projects p ON p.id = e.project_id
         LEFT JOIN integration_apps a ON a.id = p.app_id
         WHERE e.org_id = $1 AND ($2::uuid IS NULL OR e.id > $2) AND ($3::text IS NULL OR a.app_key = $3)
         ORDER BY e.id LIMIT $4",
    )
    .bind(org_id)
    .bind(q.after)
    .bind(&q.app)
    .bind(limit * 4) // headroom for the type filter below
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;

    let mut items = Vec::new();
    let mut last_seen = q.after;
    for (id, event_type) in ids {
        if items.len() as i64 >= limit {
            break;
        }
        last_seen = Some(id);
        if let Some(p) = &q.event_type {
            if !events::pattern_matches(p, &event_type) {
                continue;
            }
        }
        if let Some(env) = envelope(&state.db, id).await {
            items.push(env);
        }
    }
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "events": items, "next_after": last_seen }),
    )))
}

async fn event_types(
    caller: IntegrationCaller,
) -> Result<Json<ApiResponse<Vec<&'static str>>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    Ok(Json(ApiResponse::ok(events::EVENT_TYPES.to_vec())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_reference() {
        // HMAC-SHA256("whsec_test", "1700000000.{}") computed independently.
        // Python: hmac.new(b"whsec_test", b"1700000000.{}", hashlib.sha256).hexdigest()
        assert_eq!(
            sign("whsec_test", 1_700_000_000, b"{}"),
            "t=1700000000,v1=35495024f4ef3f94e5a93e22221544c4b75e9a42300cd965ab81cb85cd994e91"
        );
        assert_ne!(
            sign("whsec_test", 1_700_000_000, b"{}"),
            sign("whsec_other", 1_700_000_000, b"{}")
        );
    }

    #[test]
    fn backoff_gives_up_after_ten_failures() {
        assert_eq!(backoff_after(1), Some(10));
        assert_eq!(backoff_after(10), Some(43200));
        assert_eq!(backoff_after(11), None);
    }
}
