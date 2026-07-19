use axum::{
    body::Bytes,
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::models::OrgBilling;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Checkout request/response types ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CheckoutRequest {
    pub tier: String,
    pub success_url: String,
    pub cancel_url: String,
}

#[derive(Debug, Serialize)]
pub struct CheckoutResponse {
    pub url: String,
}

// ─── Routers ─────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/billing/webhooks", post(stripe_webhook))
}

pub fn org_routes() -> Router<AppState> {
    Router::new()
        .route("/billing", get(get_org_billing))
        .route("/billing/checkout", post(create_checkout_session))
        .route("/billing/history", get(get_billing_history))
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn get_org_billing(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<OrgBilling>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let billing: Option<OrgBilling> = sqlx::query_as::<_, OrgBilling>(
        r#"SELECT org_id, stripe_customer_id, stripe_sub_id, tier::text AS tier,
                  sub_status, current_period_end, updated_at
           FROM org_billing
           WHERE org_id = $1"#,
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let billing = match billing {
        Some(b) => b,
        None => {
            sqlx::query_as::<_, OrgBilling>(
                r#"INSERT INTO org_billing (org_id, tier, sub_status, updated_at)
                   VALUES ($1, 'free', 'active', NOW())
                   ON CONFLICT (org_id) DO UPDATE SET updated_at = org_billing.updated_at
                   RETURNING org_id, stripe_customer_id, stripe_sub_id, tier::text AS tier,
                             sub_status, current_period_end, updated_at"#,
            )
            .bind(org_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        }
    };

    Ok(Json(ApiResponse::ok(billing)))
}

async fn create_checkout_session(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Json(body): Json<CheckoutRequest>,
) -> Result<Json<ApiResponse<CheckoutResponse>>, ApiAppError> {
    crate::orgs::require_admin(&state.db, org_id, auth.user_id).await?;

    let secret_key = state.config.stripe_secret_key.as_deref().ok_or_else(|| {
        ApiAppError(AppError::Internal("Stripe is not configured on this server".to_string()))
    })?;

    if body.tier != "pro" && body.tier != "max" {
        return Err(ApiAppError(AppError::Validation("tier must be 'pro' or 'max'".to_string())));
    }

    let price_id = match body.tier.as_str() {
        "pro" => state.config.stripe_price_pro.clone().unwrap_or_default(),
        "max" => state.config.stripe_price_max.clone().unwrap_or_default(),
        _ => unreachable!(),
    };

    if price_id.is_empty() {
        return Err(ApiAppError(AppError::Internal(
            "Stripe price ID not configured for this tier".to_string(),
        )));
    }

    let org_id_str = org_id.to_string();
    let quantity_str = "1";

    // Call Stripe API — create a checkout session.
    let params = [
        ("mode", "subscription"),
        ("line_items[0][price]", price_id.as_str()),
        ("line_items[0][quantity]", quantity_str),
        ("success_url", body.success_url.as_str()),
        ("cancel_url", body.cancel_url.as_str()),
        ("metadata[org_id]", org_id_str.as_str()),
        ("metadata[tier]", body.tier.as_str()),
    ];

    let res = state.http_client
        .post("https://api.stripe.com/v1/checkout/sessions")
        .basic_auth(secret_key, Some(""))
        .form(&params)
        .send()
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("Stripe request failed: {e}"))))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(ApiAppError(AppError::Internal(format!(
            "Stripe returned {}: {}",
            status, text
        ))));
    }

    let session: serde_json::Value = res.json().await
        .map_err(|e| ApiAppError(AppError::Internal(format!("Stripe response parse: {e}"))))?;

    let url = session["url"]
        .as_str()
        .ok_or_else(|| ApiAppError(AppError::Internal("Stripe session missing url".to_string())))?
        .to_string();

    Ok(Json(ApiResponse::ok(CheckoutResponse { url })))
}

// ─── Billing history ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PaymentRecord {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub plan_id: Option<Uuid>,
    pub plan_name: Option<String>,
    pub stripe_payment_intent_id: Option<String>,
    pub amount: i32,
    pub currency: String,
    pub status: String,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn get_billing_history(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<PaymentRecord>>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let records: Vec<PaymentRecord> = sqlx::query_as::<_, PaymentRecord>(
        r#"SELECT p.id, p.org_id, p.plan_id, pl.name AS plan_name,
                  p.stripe_payment_intent_id, p.amount, p.currency,
                  p.status, p.description, p.created_at
           FROM payments p
           LEFT JOIN plans pl ON pl.id = p.plan_id
           WHERE p.org_id = $1
           ORDER BY p.created_at DESC
           LIMIT 100"#,
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(records)))
}

async fn stripe_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    if let Some(secret) = &state.config.stripe_webhook_secret {
        let sig_header = headers
            .get("stripe-signature")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ApiAppError(AppError::Unauthorized(
                "Missing Stripe-Signature header".to_string(),
            )))?;

        if !verify_stripe_signature(secret, &body, sig_header) {
            return Err(ApiAppError(AppError::Unauthorized(
                "Stripe webhook signature verification failed".to_string(),
            )));
        }
    }

    let event: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {e}"))))?;

    let event_type = event["type"].as_str().unwrap_or_default();
    let obj = &event["data"]["object"];

    match event_type {
        "checkout.session.completed" => {
            let customer_id = obj["customer"].as_str().unwrap_or_default();
            let sub_id      = obj["subscription"].as_str().unwrap_or_default();
            // Read org_id and tier from the metadata we embed at checkout time.
            let org_id_str  = obj["metadata"]["org_id"].as_str().unwrap_or_default();
            let tier        = obj["metadata"]["tier"].as_str().unwrap_or("pro");

            let org_id = match org_id_str.parse::<Uuid>() {
                Ok(id) => id,
                Err(_) => {
                    tracing::warn!("stripe: checkout.session.completed missing valid org_id in metadata");
                    return Ok(Json(ApiResponse::ok(serde_json::json!({ "received": true }))));
                }
            };

            // 1. Resolve plan_id for this tier.
            let plan_id: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM plans WHERE name = $1 LIMIT 1",
            )
            .bind(tier)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            // 2. Upsert org_billing with the correct tier and plan_id.
            sqlx::query(
                r#"INSERT INTO org_billing (org_id, stripe_customer_id, stripe_sub_id, tier, plan_id, sub_status, updated_at)
                   VALUES ($1, $2, $3, $4::subscription_tier, $5, 'active', NOW())
                   ON CONFLICT (org_id) DO UPDATE
                       SET stripe_customer_id = EXCLUDED.stripe_customer_id,
                           stripe_sub_id      = EXCLUDED.stripe_sub_id,
                           tier               = EXCLUDED.tier,
                           plan_id            = EXCLUDED.plan_id,
                           sub_status         = 'active',
                           updated_at         = NOW()"#,
            )
            .bind(org_id)
            .bind(customer_id)
            .bind(sub_id)
            .bind(tier)
            .bind(plan_id)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            // 3. Sync organizations.plan_id to match.
            if let Some(pid) = plan_id {
                sqlx::query(
                    "UPDATE organizations SET plan_id = $2 WHERE id = $1",
                )
                .bind(org_id)
                .bind(pid)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

                // 4. Snapshot plan limits into org_quota for this org.
                // This is an explicit plan upgrade — overwrite with new limits.
                sqlx::query(
                    r#"INSERT INTO org_quota (
                           org_id, plan_id,
                           max_projects, max_members, max_replicas, max_parallel_deployments,
                           max_git_providers, max_orgs, node_count, cpu_cores, memory_mb,
                           applied_at, updated_at
                       )
                       SELECT $1, id,
                           max_projects, max_members, max_replicas, max_parallel_deployments,
                           max_git_providers, max_orgs, node_count, cpu_cores, memory_mb,
                           NOW(), NOW()
                       FROM plans WHERE id = $2
                       ON CONFLICT (org_id) DO UPDATE
                           SET plan_id = EXCLUDED.plan_id,
                               max_projects = EXCLUDED.max_projects,
                               max_members = EXCLUDED.max_members,
                               max_replicas = EXCLUDED.max_replicas,
                               max_parallel_deployments = EXCLUDED.max_parallel_deployments,
                               max_git_providers = EXCLUDED.max_git_providers,
                               max_orgs = EXCLUDED.max_orgs,
                               node_count = EXCLUDED.node_count,
                               cpu_cores = EXCLUDED.cpu_cores,
                               memory_mb = EXCLUDED.memory_mb,
                               applied_at = NOW(),
                               updated_at = NOW()"#,
                )
                .bind(org_id)
                .bind(pid)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
            }

            // 4. Record the payment.
            let amount_total = obj["amount_total"].as_i64().unwrap_or(0) as i32;
            let currency = obj["currency"].as_str().unwrap_or("usd").to_string();
            let payment_intent = obj["payment_intent"].as_str();
            sqlx::query(
                r#"INSERT INTO payments (id, org_id, plan_id, stripe_payment_intent_id, amount, currency, status, description, created_at)
                   VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, 'succeeded', $6, NOW())
                   ON CONFLICT (stripe_payment_intent_id) DO NOTHING"#,
            )
            .bind(org_id)
            .bind(plan_id)
            .bind(payment_intent)
            .bind(amount_total)
            .bind(&currency)
            .bind(format!("Subscription checkout — {tier} plan"))
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            // 2. Look up the org name to build a node name.
            let org_name: Option<String> = sqlx::query_scalar(
                "SELECT name FROM organizations WHERE id = $1",
            )
            .bind(org_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            let node_name = org_name
                .as_deref()
                .map(|n| format!("{}-node-1", n.to_lowercase().replace(' ', "-")))
                .unwrap_or_else(|| format!("org-{}-node-1", org_id));

            // 3. Create a compute_nodes row to trigger provisioning (idempotent check).
            let existing: Option<(Uuid,)> = sqlx::query_as(
                "SELECT id FROM compute_nodes WHERE org_id = $1 AND status = 'provisioning'::node_status LIMIT 1",
            )
            .bind(org_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            if existing.is_none() {
                let provider = &state.config.default_cloud_provider;
                let region = if provider == "digitalocean" {
                    &state.config.do_region
                } else {
                    &state.config.hetzner_region
                };
                sqlx::query(
                    r#"INSERT INTO compute_nodes
                           (id, org_id, name, provider, region, status, cpu_cores, ram_mb, provision_attempts, created_at, updated_at)
                       VALUES (gen_random_uuid(), $1, $2, $3, $4, 'provisioning'::node_status, 2, 4096, 0, NOW(), NOW())"#,
                )
                .bind(org_id)
                .bind(&node_name)
                .bind(provider)
                .bind(region)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
            }

            tracing::info!(org_id = %org_id, tier = %tier, "checkout completed — node provisioning queued");
        }
        "customer.subscription.updated" => {
            let customer_id = obj["customer"].as_str().unwrap_or_default();
            let sub_id = obj["id"].as_str().unwrap_or_default();
            let status = obj["status"].as_str().unwrap_or("active");
            let period_end = obj["current_period_end"]
                .as_i64()
                .map(|ts| chrono::DateTime::from_timestamp(ts, 0))
                .flatten();

            // Determine new tier from current price ID (if recognizable).
            let price_id = obj["items"]["data"][0]["price"]["id"].as_str().unwrap_or_default();
            let tier_opt: Option<&str> = if !price_id.is_empty() {
                let pro = state.config.stripe_price_pro.as_deref().unwrap_or("");
                let max = state.config.stripe_price_max.as_deref().unwrap_or("");
                if !pro.is_empty() && price_id == pro {
                    Some("pro")
                } else if !max.is_empty() && price_id == max {
                    Some("max")
                } else {
                    None
                }
            } else {
                None
            };

            if let Some(tier) = tier_opt {
                sqlx::query(
                    r#"UPDATE org_billing
                       SET stripe_sub_id = $2, sub_status = $3, current_period_end = $4,
                           tier = $5::subscription_tier, updated_at = NOW()
                       WHERE stripe_customer_id = $1"#,
                )
                .bind(customer_id)
                .bind(sub_id)
                .bind(status)
                .bind(period_end)
                .bind(tier)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
            } else {
                sqlx::query(
                    r#"UPDATE org_billing
                       SET stripe_sub_id = $2, sub_status = $3, current_period_end = $4, updated_at = NOW()
                       WHERE stripe_customer_id = $1"#,
                )
                .bind(customer_id)
                .bind(sub_id)
                .bind(status)
                .bind(period_end)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
            }
        }
        "customer.subscription.deleted" => {
            let customer_id = obj["customer"].as_str().unwrap_or_default();

            sqlx::query(
                r#"UPDATE org_billing
                   SET sub_status = 'canceled', updated_at = NOW()
                   WHERE stripe_customer_id = $1"#,
            )
            .bind(customer_id)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            // Mark all active/degraded nodes for this org as stopped.
            sqlx::query(
                r#"UPDATE compute_nodes cn
                   SET status = 'stopped'::node_status, updated_at = NOW()
                   FROM org_billing ob
                   WHERE ob.stripe_customer_id = $1
                     AND cn.org_id = ob.org_id
                     AND cn.status IN ('active'::node_status, 'degraded'::node_status)"#,
            )
            .bind(customer_id)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
        }
        "invoice.paid" => {
            // Recurring invoice paid — record in payments and keep org_billing current.
            let customer_id = obj["customer"].as_str().unwrap_or_default();
            let payment_intent = obj["payment_intent"].as_str();
            let amount = obj["amount_paid"].as_i64().unwrap_or(0) as i32;
            let currency = obj["currency"].as_str().unwrap_or("usd").to_string();

            // Look up the org for this customer.
            let org_id_opt: Option<Uuid> = sqlx::query_scalar(
                "SELECT org_id FROM org_billing WHERE stripe_customer_id = $1",
            )
            .bind(customer_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            if let Some(org_id) = org_id_opt {
                let plan_id: Option<Uuid> = sqlx::query_scalar(
                    "SELECT plan_id FROM org_billing WHERE org_id = $1",
                )
                .bind(org_id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None)
                .flatten();

                sqlx::query(
                    r#"INSERT INTO payments (id, org_id, plan_id, stripe_payment_intent_id, amount, currency, status, description, created_at)
                       VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, 'succeeded', 'Recurring subscription payment', NOW())
                       ON CONFLICT (stripe_payment_intent_id) DO NOTHING"#,
                )
                .bind(org_id)
                .bind(plan_id)
                .bind(payment_intent)
                .bind(amount)
                .bind(&currency)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

                sqlx::query(
                    "UPDATE org_billing SET sub_status = 'active', updated_at = NOW() WHERE org_id = $1",
                )
                .bind(org_id)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
            }
        }
        "invoice.payment_failed" => {
            let customer_id = obj["customer"].as_str().unwrap_or_default();

            let org_id_opt: Option<Uuid> = sqlx::query_scalar(
                "SELECT org_id FROM org_billing WHERE stripe_customer_id = $1",
            )
            .bind(customer_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

            sqlx::query(
                r#"UPDATE org_billing
                   SET sub_status = 'past_due', updated_at = NOW()
                   WHERE stripe_customer_id = $1"#,
            )
            .bind(customer_id)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            // Record the failed payment attempt.
            if let Some(org_id) = org_id_opt {
                let payment_intent = obj["payment_intent"].as_str();
                let amount = obj["amount_due"].as_i64().unwrap_or(0) as i32;
                let currency = obj["currency"].as_str().unwrap_or("usd").to_string();
                let plan_id: Option<Uuid> = sqlx::query_scalar(
                    "SELECT plan_id FROM org_billing WHERE org_id = $1",
                )
                .bind(org_id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or(None)
                .flatten();

                sqlx::query(
                    r#"INSERT INTO payments (id, org_id, plan_id, stripe_payment_intent_id, amount, currency, status, description, created_at)
                       VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, 'failed', 'Subscription payment failed', NOW())
                       ON CONFLICT (stripe_payment_intent_id) DO NOTHING"#,
                )
                .bind(org_id)
                .bind(plan_id)
                .bind(payment_intent)
                .bind(amount)
                .bind(&currency)
                .execute(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
            }
        }
        _ => {
            tracing::debug!("stripe: unhandled event type '{event_type}'");
        }
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "received": true }))))
}

// ─── Stripe signature verification ───────────────────────────────────────────

fn verify_stripe_signature(secret: &str, body: &[u8], sig_header: &str) -> bool {
    // Stripe-Signature: t=<timestamp>,v1=<hex_sig>[,v1=<hex_sig>...]
    let timestamp = sig_header
        .split(',')
        .find_map(|part| part.strip_prefix("t="));

    let signatures: Vec<&str> = sig_header
        .split(',')
        .filter_map(|part| part.strip_prefix("v1="))
        .collect();

    let timestamp = match timestamp {
        Some(t) => t,
        None => return false,
    };

    if signatures.is_empty() {
        return false;
    }

    // Signed payload = "<timestamp>.<body>"
    let mut signed_payload = Vec::with_capacity(timestamp.len() + 1 + body.len());
    signed_payload.extend_from_slice(timestamp.as_bytes());
    signed_payload.push(b'.');
    signed_payload.extend_from_slice(body);

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(&signed_payload);
    let computed = mac.finalize().into_bytes();
    let computed_hex = hex::encode(computed);

    signatures.iter().any(|&sig| sig == computed_hex)
}
