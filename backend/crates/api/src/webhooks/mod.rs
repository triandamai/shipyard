//! Webhook receivers for Git providers.
//!
//! Routes (no auth — the token in the URL is the authentication mechanism):
//!
//!   POST /webhooks/github/:service_id/:token
//!   POST /webhooks/gitlab/:service_id/:token
//!   POST /webhooks/gitea/:service_id/:token
//!
//! On a matching push event the handler triggers a deployment for the service,
//! identical to `POST /projects/:project_id/services/:service_id/deploy`.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_engine::DeploymentEngine;

use crate::error::ApiAppError;
use crate::AppState;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/github/:service_id/:token", post(github_webhook))
        .route("/gitlab/:service_id/:token", post(gitlab_webhook))
        .route("/gitea/:service_id/:token", post(gitea_webhook))
        // Edge function group webhooks
        .route("/github/fn/:group_id/:token", post(github_fn_webhook))
        .route("/gitlab/fn/:group_id/:token", post(gitlab_fn_webhook))
}

// ─── Simple Glob Matcher ───────────────────────────────────────────────────────

fn matches_pattern(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.starts_with('*') && pattern.ends_with('*') {
        let sub = &pattern[1..pattern.len() - 1];
        return text.contains(sub);
    } else if pattern.starts_with('*') {
        let sub = &pattern[1..];
        return text.ends_with(sub);
    } else if pattern.ends_with('*') {
        let sub = &pattern[..pattern.len() - 1];
        return text.starts_with(sub);
    }
    if let Some(star_idx) = pattern.find('*') {
        let prefix = &pattern[..star_idx];
        let suffix = &pattern[star_idx + 1..];
        return text.starts_with(prefix) && text.ends_with(suffix) && text.len() >= (prefix.len() + suffix.len());
    }
    pattern == text
}

// ─── GitHub ───────────────────────────────────────────────────────────────────

/// POST /webhooks/github/:service_id/:token
async fn github_webhook(
    Path((service_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    // 1. Load webhook token from service_envs
    let webhook_secret = load_webhook_token(&state, service_id).await?;

    // 2. Verify token in URL
    if token != webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized(
            "Invalid webhook token".to_string(),
        )));
    }

    // 3. Optional HMAC-SHA256 signature verification (X-Hub-Signature-256)
    let sig_header = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok());

    if !verify_github_signature(&webhook_secret, &body, sig_header) {
        return Err(ApiAppError(AppError::Unauthorized(
            "Webhook signature verification failed".to_string(),
        )));
    }

    // 4. Parse the payload
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let payload: serde_json::Value = if content_type.contains("application/x-www-form-urlencoded") {
        let body_str = String::from_utf8(body.to_vec())
            .map_err(|_| ApiAppError(AppError::BadRequest("webhook body is not valid UTF-8".to_string())))?;
        let json_str = body_str
            .split('&')
            .find_map(|part| {
                let (k, v) = part.split_once('=')?;
                if urlencoding::decode(k).ok()?.as_ref() == "payload" {
                    urlencoding::decode(&v.replace('+', " ")).ok().map(|s| s.into_owned())
                } else {
                    None
                }
            })
            .ok_or_else(|| ApiAppError(AppError::BadRequest("missing payload field in form-encoded webhook body".to_string())))?;
        serde_json::from_str(&json_str)
            .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON in form payload: {}", e))))?
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {}", e))))?
    };

    let event_type_opt = headers.get("x-github-event").and_then(|v| v.to_str().ok());

    let svc_row: Option<(String, String, bool, String, Option<String>, Option<String>)> = sqlx::query_as::<_, (String, String, bool, String, Option<String>, Option<String>)>(
        "SELECT type::text, git_branch, auto_deploy, git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (service_type, services_git_branch, auto_deploy, svc_strategy, svc_git_deploy_branch, svc_git_deploy_tag_pattern) = match svc_row {
        Some(row) => row,
        None => return Err(ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id)))),
    };

    if !auto_deploy {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "auto-deploy disabled for this service",
        }))));
    }

    let (strategy, git_deploy_branch, git_deploy_tag_pattern) = if service_type == "static" {
        let static_row = sqlx::query_as::<_, (String, Option<String>, Option<String>)>(
            "SELECT git_deploy_strategy, git_deploy_branch, git_deploy_tag_pattern
             FROM static_site_configs WHERE service_id = $1",
        )
        .bind(service_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        match static_row {
            Some(row) => row,
            None => ("push".to_string(), None, None),
        }
    } else {
        (svc_strategy, svc_git_deploy_branch, svc_git_deploy_tag_pattern)
    };

    let target_branch = git_deploy_branch.as_deref().unwrap_or(&services_git_branch);

    if event_type_opt.is_none() {
        // Manual trigger (copied URL, curl, etc.)
        let pushed_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or_default();
        let target = if pushed_ref.is_empty() {
            target_branch
        } else {
            branch_from_ref(pushed_ref)
        };
        return trigger_deploy(&state, service_id, target).await;
    }

    let event_type = event_type_opt.unwrap_or("push");

    match strategy.as_str() {
        "push" => {
            if event_type != "push" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "ignored event type (expected push strategy)",
                }))));
            }
            let pushed_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or_default();
            if pushed_ref.starts_with("refs/tags/") {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "ignored non-branch push",
                }))));
            }
            let pushed_branch = branch_from_ref(pushed_ref);
            if pushed_branch != target_branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "branch mismatch",
                    "pushed": pushed_branch,
                    "target": target_branch,
                }))));
            }
            trigger_deploy(&state, service_id, target_branch).await
        }
        "tag" => {
            if event_type != "push" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "ignored event type (expected push for tag strategy)",
                }))));
            }
            let pushed_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or_default();
            if !pushed_ref.starts_with("refs/tags/") {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "ignored non-tag push",
                }))));
            }
            let tag_name = pushed_ref.strip_prefix("refs/tags/").unwrap_or(pushed_ref);
            if let Some(pattern) = &git_deploy_tag_pattern {
                if !matches_pattern(pattern, tag_name) {
                    return Ok(Json(ApiResponse::ok(serde_json::json!({
                        "message": "tag pattern mismatch",
                        "tag": tag_name,
                        "pattern": pattern,
                    }))));
                }
            }
            trigger_deploy(&state, service_id, tag_name).await
        }
        "pull_request" => {
            if event_type != "pull_request" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "ignored event type (expected pull_request)",
                }))));
            }
            let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or_default();
            let merged = payload.get("pull_request").and_then(|pr| pr.get("merged")).and_then(|m| m.as_bool()).unwrap_or(false);
            if action != "closed" || !merged {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "ignored pull request event (must be closed and merged)",
                    "action": action,
                    "merged": merged,
                }))));
            }
            let base_ref = payload
                .get("pull_request")
                .and_then(|pr| pr.get("base"))
                .and_then(|b| b.get("ref"))
                .and_then(|r| r.as_str())
                .unwrap_or_default();
            
            if base_ref != target_branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "pull request target branch mismatch",
                    "base_ref": base_ref,
                    "target": target_branch,
                }))));
            }
            
            let merge_sha = payload
                .get("pull_request")
                .and_then(|pr| pr.get("merge_commit_sha"))
                .and_then(|s| s.as_str())
                .unwrap_or(target_branch);

            trigger_deploy(&state, service_id, merge_sha).await
        }
        _ => {
            let pushed_ref = payload.get("ref").and_then(|v| v.as_str()).unwrap_or_default();
            let pushed_branch = branch_from_ref(pushed_ref);
            if pushed_branch != target_branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({
                    "message": "branch mismatch (unknown strategy fallback)",
                    "pushed": pushed_branch,
                    "target": target_branch,
                }))));
            }
            trigger_deploy(&state, service_id, target_branch).await
        }
    }
}

// ─── GitLab ───────────────────────────────────────────────────────────────────

/// POST /webhooks/gitlab/:service_id/:token
async fn gitlab_webhook(
    Path((service_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    _headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let webhook_secret = load_webhook_token(&state, service_id).await?;

    if token != webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized(
            "Invalid webhook token".to_string(),
        )));
    }

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {}", e))))?;

    let kind = payload
        .get("object_kind")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if kind != "push" {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "ignored non-push event",
            "object_kind": kind,
        }))));
    }

    let pushed_ref = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let pushed_branch = branch_from_ref(pushed_ref);

    let svc_row: Option<(String, bool)> = sqlx::query_as::<_, (String, bool)>(
        "SELECT git_branch, auto_deploy FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (configured_branch, auto_deploy) = match svc_row {
        Some(row) => row,
        None => return Err(ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id)))),
    };

    if !auto_deploy {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "auto-deploy disabled for this service",
        }))));
    }

    if configured_branch != pushed_branch {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "push ignored: branch mismatch",
            "pushed": pushed_branch,
            "configured": configured_branch,
        }))));
    }

    trigger_deploy(&state, service_id, pushed_branch).await
}

// ─── Gitea ────────────────────────────────────────────────────────────────────

/// POST /webhooks/gitea/:service_id/:token
async fn gitea_webhook(
    Path((service_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    _headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let webhook_secret = load_webhook_token(&state, service_id).await?;

    if token != webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized(
            "Invalid webhook token".to_string(),
        )));
    }

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {}", e))))?;

    let pushed_ref = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let pushed_branch = branch_from_ref(pushed_ref);

    let svc_row: Option<(String, bool)> = sqlx::query_as::<_, (String, bool)>(
        "SELECT git_branch, auto_deploy FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (configured_branch, auto_deploy) = match svc_row {
        Some(row) => row,
        None => return Err(ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id)))),
    };

    if !auto_deploy {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "auto-deploy disabled for this service",
        }))));
    }

    if configured_branch != pushed_branch {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "push ignored: branch mismatch",
            "pushed": pushed_branch,
            "configured": configured_branch,
        }))));
    }

    trigger_deploy(&state, service_id, pushed_branch).await
}

// ─── Shared logic ─────────────────────────────────────────────────────────────

fn branch_from_ref(git_ref: &str) -> &str {
    git_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(git_ref)
}

async fn load_webhook_token(state: &AppState, service_id: Uuid) -> Result<String, ApiAppError> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT value_encrypted FROM service_envs
         WHERE service_id = $1 AND key = '__WEBHOOK_TOKEN__'
         LIMIT 1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let stored = row.map(|(v,)| v).ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!(
            "No webhook token configured for service '{service_id}'"
        )))
    })?;

    Ok(shipyard_common::crypto::decrypt_or_passthrough(
        &state.config.auth.secret_key,
        &stored,
    ))
}

async fn trigger_deploy(
    state: &AppState,
    service_id: Uuid,
    source_ref: &str,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let source_ref = source_ref.to_string();

    let max_parallel = sqlx::query_as::<_, (String,)>(
        "SELECT value::text FROM system_config WHERE key = 'max_parallel_deployments'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .and_then(|(v,)| v.trim_matches('"').parse::<i64>().ok())
    .unwrap_or(2);

    let mut is_queued = false;
    if max_parallel > 0 {
        let running: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM deployments WHERE status = 'running'::deployment_status",
        )
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        if running.0 >= max_parallel {
            is_queued = true;
        }
    }

    let deployment_id = uuid::Uuid::now_v7();
    let status = if is_queued { "queued" } else { "running" };

    sqlx::query(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         VALUES ($1, $2, 'webhook', $3, $4::deployment_status, NOW())",
    )
    .bind(deployment_id)
    .bind(service_id)
    .bind(&source_ref)
    .bind(status)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if is_queued {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "webhook deployment queued",
            "deployment_id": deployment_id,
        }))));
    }

    let engine = DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
        state.config.data_dir.clone(),
        state.config.static_server.retention_versions,
    );

    let webhook_notify = Arc::clone(&state.swarm_sync_trigger);
    let source_ref_cp = source_ref.clone();
    tokio::spawn(async move {
        if let Err(e) = engine.deploy(deployment_id, service_id, "webhook", &source_ref_cp).await {
            tracing::error!(
                service_id = %service_id,
                deployment_id = %deployment_id,
                error = %e,
                "webhook-triggered deployment failed"
            );
        }
        webhook_notify.notify_one();
    });

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "deployment triggered",
        "service_id": service_id,
        "ref": source_ref,
    }))))
}

pub fn verify_github_signature(secret: &str, body: &[u8], signature: Option<&str>) -> bool {
    let sig_str = match signature {
        Some(s) => s,
        None => return true,
    };

    let expected_hex = match sig_str.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };

    let expected_bytes = match hex::decode(expected_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);

    mac.verify_slice(&expected_bytes).is_ok()
}

// ─── Edge function group webhooks ─────────────────────────────────────────────

/// POST /webhooks/github/fn/:group_id/:token
async fn github_fn_webhook(
    Path((group_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let group = load_fn_group(&state, group_id, &token).await?;

    let sig = headers.get("x-hub-signature-256").and_then(|v| v.to_str().ok());
    if !verify_github_signature(&group.webhook_secret, &body, sig) {
        return Err(ApiAppError(AppError::Unauthorized("signature mismatch".into())));
    }

    if !group.auto_deploy {
        return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "auto-deploy disabled" }))));
    }

    let event = headers.get("x-github-event").and_then(|v| v.to_str().ok()).unwrap_or("");

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {e}"))))?;

    let commit_sha = match group.deploy_strategy.as_str() {
        "push" => {
            if event != "push" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored event (expected push)" }))));
            }
            let pushed_ref = payload["ref"].as_str().unwrap_or("");
            if pushed_ref.starts_with("refs/tags/") {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored tag push" }))));
            }
            let pushed_branch = pushed_ref.strip_prefix("refs/heads/").unwrap_or(pushed_ref);
            if pushed_branch != group.branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "branch not tracked" }))));
            }
            payload["after"].as_str().unwrap_or("").to_string()
        }
        "tag" => {
            if event != "push" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored event (expected push for tag)" }))));
            }
            let pushed_ref = payload["ref"].as_str().unwrap_or("");
            if !pushed_ref.starts_with("refs/tags/") {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored non-tag push" }))));
            }
            let tag = pushed_ref.strip_prefix("refs/tags/").unwrap_or(pushed_ref);
            if let Some(pattern) = &group.deploy_tag_pattern {
                if !matches_pattern(pattern, tag) {
                    return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "tag pattern mismatch" }))));
                }
            }
            payload["after"].as_str().unwrap_or("").to_string()
        }
        "pull_request" => {
            if event != "pull_request" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored event (expected pull_request)" }))));
            }
            let action = payload["action"].as_str().unwrap_or("");
            let merged = payload["pull_request"]["merged"].as_bool().unwrap_or(false);
            if action != "closed" || !merged {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored (must be closed+merged)" }))));
            }
            let base_ref = payload["pull_request"]["base"]["ref"].as_str().unwrap_or("");
            if base_ref != group.branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "PR target branch mismatch" }))));
            }
            payload["pull_request"]["merge_commit_sha"].as_str().unwrap_or("").to_string()
        }
        _ => {
            // Unknown strategy — fall back to push behaviour.
            let pushed_branch = payload["ref"].as_str()
                .and_then(|r| r.strip_prefix("refs/heads/"))
                .unwrap_or("");
            if pushed_branch != group.branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "branch not tracked" }))));
            }
            payload["after"].as_str().unwrap_or("").to_string()
        }
    };

    tokio::spawn(async move {
        if let Err(e) = crate::edge_functions::manager::deploy_from_git(&state, &group, &commit_sha, None).await {
            tracing::error!(group_id = %group.id, "edge fn deploy failed: {e}");
        }
    });

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "deploy triggered" }))))
}

/// POST /webhooks/gitlab/fn/:group_id/:token
async fn gitlab_fn_webhook(
    Path((group_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let group = load_fn_group(&state, group_id, &token).await?;

    // GitLab sends the secret as X-Gitlab-Token header
    let provided = headers.get("x-gitlab-token").and_then(|v| v.to_str().ok()).unwrap_or("");
    if provided != group.webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized("token mismatch".into())));
    }

    if !group.auto_deploy {
        return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "auto-deploy disabled" }))));
    }

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {e}"))))?;

    let object_kind = payload["object_kind"].as_str().unwrap_or("");

    let commit_sha = match group.deploy_strategy.as_str() {
        "push" => {
            if object_kind != "push" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored event (expected push)" }))));
            }
            let pushed_ref = payload["ref"].as_str().unwrap_or("");
            if pushed_ref.starts_with("refs/tags/") {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored tag push" }))));
            }
            let pushed_branch = pushed_ref.strip_prefix("refs/heads/").unwrap_or(pushed_ref);
            if pushed_branch != group.branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "branch not tracked" }))));
            }
            payload["checkout_sha"].as_str().unwrap_or("").to_string()
        }
        "tag" => {
            if object_kind != "tag_push" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored event (expected tag_push)" }))));
            }
            let tag = payload["ref"].as_str()
                .and_then(|r| r.strip_prefix("refs/tags/"))
                .unwrap_or("");
            if let Some(pattern) = &group.deploy_tag_pattern {
                if !matches_pattern(pattern, tag) {
                    return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "tag pattern mismatch" }))));
                }
            }
            payload["checkout_sha"].as_str().unwrap_or("").to_string()
        }
        "pull_request" => {
            // GitLab calls these merge requests; object_kind = "merge_request"
            if object_kind != "merge_request" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored event (expected merge_request)" }))));
            }
            let action = payload["object_attributes"]["action"].as_str().unwrap_or("");
            let state_val = payload["object_attributes"]["state"].as_str().unwrap_or("");
            if action != "merge" && state_val != "merged" {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "ignored (must be merged)" }))));
            }
            let target_branch = payload["object_attributes"]["target_branch"].as_str().unwrap_or("");
            if target_branch != group.branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "MR target branch mismatch" }))));
            }
            payload["object_attributes"]["merge_commit_sha"].as_str().unwrap_or("").to_string()
        }
        _ => {
            let pushed_branch = payload["ref"].as_str()
                .and_then(|r| r.strip_prefix("refs/heads/"))
                .unwrap_or("");
            if pushed_branch != group.branch {
                return Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "branch not tracked" }))));
            }
            payload["checkout_sha"].as_str().unwrap_or("").to_string()
        }
    };

    tokio::spawn(async move {
        if let Err(e) = crate::edge_functions::manager::deploy_from_git(&state, &group, &commit_sha, None).await {
            tracing::error!(group_id = %group.id, "edge fn deploy failed: {e}");
        }
    });

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "deploy triggered" }))))
}

async fn load_fn_group(
    state: &AppState,
    group_id: Uuid,
    token: &str,
) -> Result<crate::edge_functions::models::EdgeFunctionGroup, ApiAppError> {
    let group: Option<crate::edge_functions::models::EdgeFunctionGroup> = sqlx::query_as(
        "SELECT id, org_id, project_id, provider, repo_url, branch, webhook_secret,
                last_deployed_sha, service_id,
                auto_deploy, deploy_strategy, deploy_tag_pattern,
                created_at
         FROM edge_function_groups WHERE id = $1"
    )
    .bind(group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let group = group.ok_or_else(|| ApiAppError(AppError::NotFound("group not found".into())))?;

    if token != group.webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized("invalid token".into())));
    }

    Ok(group)
}

impl From<(StatusCode, Json<ApiResponse<serde_json::Value>>)> for ApiAppError {
    fn from(_: (StatusCode, Json<ApiResponse<serde_json::Value>>)) -> Self {
        ApiAppError(AppError::Internal("unexpected conversion".to_string()))
    }
}
