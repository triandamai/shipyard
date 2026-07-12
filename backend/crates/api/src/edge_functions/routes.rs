use axum::{
    extract::{Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;
use std::fmt::Write as FmtWrite;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

use super::manager;
use super::models::{
    CreateEdgeFunctionDomainRequest, CreateFunctionGroupRequest, DeployReport, EdgeFunctionDomain,
    EdgeFunctionGroup, EdgeFunctionResponse, FunctionManifestEntry, UpdateFunctionRequest,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_functions))
        .route("/:fn_id", get(get_function))
        .route("/:fn_id", put(update_function))
        .route("/:fn_id", delete(delete_function))
        .route("/upload", post(upload_zip))
        .route("/:fn_id/rollback/:dep_id", post(rollback))
        .route("/:fn_id/deployments", get(list_deployments))
        .route("/:fn_id/logs", get(list_logs))
        .route("/:fn_id/code", get(get_function_code))
        .route("/groups", get(list_groups))
        .route("/groups", post(create_group))
        .route("/groups/:group_id", delete(delete_group))
        .route("/groups/:group_id/deploy", post(trigger_group_deploy))
        .route("/groups/:group_id/domains", get(list_group_domains).post(create_group_domain))
        .route("/groups/:group_id/domains/:domain_id", delete(delete_group_domain))
}

pub fn internal_routes() -> Router<AppState> {
    Router::new()
        .route("/edge-runtime/manifest", get(runtime_manifest))
        .route("/edge-runtime/log", post(runtime_log))
}

// ─── List functions ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListFunctionsQuery {
    group_id: Option<Uuid>,
}

async fn list_functions(
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Query(q): Query<ListFunctionsQuery>,
    auth: AuthUser,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Vec<EdgeFunctionResponse>>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        org_id: Uuid,
        name: String,
        runtime: String,
        status: String,
        last_deployed_at: Option<chrono::DateTime<chrono::Utc>>,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let rows: Vec<Row> = if let Some(gid) = q.group_id {
        sqlx::query_as(
            "SELECT id, org_id, name, runtime, status::text as status,
                    last_deployed_at, created_at
             FROM edge_functions WHERE org_id = $1 AND group_id = $2
             ORDER BY created_at DESC",
        )
        .bind(org_id)
        .bind(gid)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
    } else {
        sqlx::query_as(
            "SELECT id, org_id, name, runtime, status::text as status,
                    last_deployed_at, created_at
             FROM edge_functions WHERE org_id = $1
             ORDER BY created_at DESC",
        )
        .bind(org_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
    };

    let app_domain = resolve_base_url(&state.config, &headers);
    let org_slug: String = sqlx::query_scalar(
        "SELECT slug FROM organizations WHERE id = $1"
    )
    .bind(org_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let items: Vec<EdgeFunctionResponse> = rows
        .into_iter()
        .map(|r| EdgeFunctionResponse {
            public_url: format!("{}/fn/{}/{}", app_domain, org_slug, r.name),
            id: r.id,
            org_id: r.org_id,
            name: r.name,
            runtime: r.runtime,
            status: r.status,
            last_deployed_at: r.last_deployed_at,
            created_at: r.created_at,
        })
        .collect();

    Ok(Json(ApiResponse::ok(items)))
}

// ─── Get function detail ──────────────────────────────────────────────────────

async fn get_function(
    State(state): State<AppState>,
    Path((org_id, fn_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        name: String,
        runtime: String,
        file_path: String,
        timeout_secs: i32,
        status: String,
        last_deployed_at: Option<chrono::DateTime<chrono::Utc>>,
        created_at: chrono::DateTime<chrono::Utc>,
        updated_at: chrono::DateTime<chrono::Utc>,
        dep_id: Option<Uuid>,
        commit_sha: Option<String>,
        dep_created_at: Option<chrono::DateTime<chrono::Utc>>,
    }

    let row: Row = sqlx::query_as(
        r#"SELECT ef.id, ef.name, ef.runtime, ef.file_path,
                  ef.timeout_secs, ef.status::text as status,
                  ef.last_deployed_at, ef.created_at, ef.updated_at,
                  efd.id as dep_id, efd.commit_sha,
                  efd.created_at as dep_created_at
           FROM edge_functions ef
           LEFT JOIN edge_function_deployments efd
             ON efd.function_id = ef.id AND efd.status = 'live'
           WHERE ef.id = $1 AND ef.org_id = $2"#
    )
    .bind(fn_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("edge function not found".into()))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "id": row.id,
        "name": row.name,
        "runtime": row.runtime,
        "file_path": row.file_path,
        "timeout_secs": row.timeout_secs,
        "status": row.status,
        "last_deployed_at": row.last_deployed_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "live_deployment": row.dep_id.map(|dep_id| serde_json::json!({
            "id": dep_id,
            "commit_sha": row.commit_sha,
            "created_at": row.dep_created_at,
        })),
    }))))
}

// ─── Update env vars / timeout ────────────────────────────────────────────────

async fn update_function(
    State(state): State<AppState>,
    Path((org_id, fn_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
    Json(body): Json<UpdateFunctionRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let env_vars = if let Some(raw) = body.env_vars {
        let secret_key = &state.config.auth.secret_key;
        let encrypted: serde_json::Map<String, serde_json::Value> = raw
            .as_object()
            .ok_or_else(|| AppError::BadRequest("env_vars must be an object".into()))?
            .iter()
            .map(|(k, v)| {
                let plain = v.as_str().unwrap_or("");
                let enc = shipyard_common::crypto::encrypt_or_passthrough(secret_key, plain);
                (k.clone(), serde_json::Value::String(enc))
            })
            .collect();
        Some(serde_json::Value::Object(encrypted))
    } else {
        None
    };

    sqlx::query(
        r#"UPDATE edge_functions
           SET env_vars      = COALESCE($1, env_vars),
               env_whitelist = COALESCE($2, env_whitelist),
               timeout_secs  = COALESCE($3, timeout_secs),
               updated_at    = NOW()
           WHERE id = $4 AND org_id = $5"#
    )
    .bind(env_vars)
    .bind(body.env_whitelist.as_deref())
    .bind(body.timeout_secs)
    .bind(fn_id)
    .bind(org_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "updated": true }))))
}

// ─── Delete function ──────────────────────────────────────────────────────────

async fn delete_function(
    State(state): State<AppState>,
    Path((org_id, fn_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    sqlx::query("DELETE FROM edge_functions WHERE id = $1 AND org_id = $2")
        .bind(fn_id)
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    let _ = manager::reload_runtime(&state, org_id).await;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}

// ─── Zip upload ───────────────────────────────────────────────────────────────

async fn upload_zip(
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<DeployReport>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    if !state.config.edge_functions.enabled {
        return Err(AppError::BadRequest("edge functions are not enabled".into()).into());
    }

    let mut zip_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            zip_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("read error: {e}")))?
                    .to_vec(),
            );
        }
    }

    let zip_bytes = zip_bytes
        .ok_or_else(|| AppError::BadRequest("missing 'file' field".into()))?;

    let tmp = tempfile::tempdir()
        .map_err(|e| AppError::Internal(format!("tempdir: {e}")))?;

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&zip_bytes))
        .map_err(|e| AppError::BadRequest(format!("invalid zip: {e}")))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::Internal(e.to_string()))?;
        let outpath = tmp.path().join(file.name());
        if file.is_dir() {
            std::fs::create_dir_all(&outpath).ok();
        } else {
            if let Some(parent) = outpath.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let mut out =
                std::fs::File::create(&outpath).map_err(|e| AppError::Internal(e.to_string()))?;
            std::io::copy(&mut file, &mut out)
                .map_err(|e| AppError::Internal(e.to_string()))?;
        }
    }

    let report =
        manager::deploy_from_path(&state, org_id, tmp.path(), None, None, Some(auth.user_id))
            .await?;

    Ok(Json(ApiResponse::ok(report)))
}

// ─── Rollback ─────────────────────────────────────────────────────────────────

async fn rollback(
    State(state): State<AppState>,
    Path((org_id, fn_id, dep_id)): Path<(Uuid, Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let code: String = sqlx::query_scalar(
        "SELECT efd.code_bundle FROM edge_function_deployments efd
         JOIN edge_functions ef ON ef.id = efd.function_id
         WHERE efd.id = $1 AND ef.org_id = $2"
    )
    .bind(dep_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("deployment not found".into()))?;

    manager::deploy_function(&state, fn_id, &code, None, Some(auth.user_id)).await?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "rolled_back": true }))))
}

// ─── Deployment history ───────────────────────────────────────────────────────

async fn list_deployments(
    State(state): State<AppState>,
    Path((org_id, fn_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        commit_sha: Option<String>,
        deployed_by: Option<Uuid>,
        status: String,
        error: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT efd.id, efd.commit_sha, efd.deployed_by, efd.status, efd.error, efd.created_at
         FROM edge_function_deployments efd
         JOIN edge_functions ef ON ef.id = efd.function_id
         WHERE efd.function_id = $1 AND ef.org_id = $2
         ORDER BY efd.created_at DESC
         LIMIT 50"
    )
    .bind(fn_id)
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| serde_json::json!({
            "id": r.id,
            "commit_sha": r.commit_sha,
            "deployed_by": r.deployed_by,
            "status": r.status,
            "error": r.error,
            "created_at": r.created_at,
        }))
        .collect();

    Ok(Json(ApiResponse::ok(serde_json::json!({ "items": items }))))
}

// ─── Invocation logs ──────────────────────────────────────────────────────────

async fn list_logs(
    State(state): State<AppState>,
    Path((org_id, fn_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        request_id: String,
        method: String,
        path: String,
        status_code: i32,
        duration_ms: i32,
        error: Option<String>,
        logged_at: chrono::DateTime<chrono::Utc>,
    }

    let rows: Vec<Row> = sqlx::query_as(
        "SELECT l.id, l.request_id, l.method, l.path, l.status_code,
                l.duration_ms, l.error, l.logged_at
         FROM edge_function_invocation_logs l
         JOIN edge_functions ef ON ef.id = l.function_id
         WHERE l.function_id = $1 AND ef.org_id = $2
         ORDER BY l.logged_at DESC
         LIMIT 100"
    )
    .bind(fn_id)
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| serde_json::json!({
            "id": r.id,
            "request_id": r.request_id,
            "method": r.method,
            "path": r.path,
            "status_code": r.status_code,
            "duration_ms": r.duration_ms,
            "error": r.error,
            "logged_at": r.logged_at,
        }))
        .collect();

    Ok(Json(ApiResponse::ok(serde_json::json!({ "items": items }))))
}

// ─── Get code bundle (live or specific deployment) ────────────────────────────

#[derive(Deserialize)]
struct CodeQuery {
    deployment_id: Option<Uuid>,
}

async fn get_function_code(
    State(state): State<AppState>,
    Path((org_id, fn_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<CodeQuery>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let code: Option<String> = if let Some(dep_id) = q.deployment_id {
        sqlx::query_scalar(
            "SELECT efd.code_bundle
             FROM edge_function_deployments efd
             JOIN edge_functions ef ON ef.id = efd.function_id
             WHERE efd.id = $1 AND efd.function_id = $2 AND ef.org_id = $3",
        )
        .bind(dep_id)
        .bind(fn_id)
        .bind(org_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .flatten()
    } else {
        sqlx::query_scalar(
            "SELECT efd.code_bundle
             FROM edge_function_deployments efd
             JOIN edge_functions ef ON ef.id = efd.function_id
             WHERE efd.function_id = $1 AND ef.org_id = $2 AND efd.status = 'live'
             ORDER BY efd.created_at DESC
             LIMIT 1",
        )
        .bind(fn_id)
        .bind(org_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?
        .flatten()
    };

    Ok(Json(ApiResponse::ok(serde_json::json!({ "code": code }))))
}

// ─── Git groups ───────────────────────────────────────────────────────────────

async fn list_groups(
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<EdgeFunctionGroup>>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let groups: Vec<EdgeFunctionGroup> = sqlx::query_as(
        "SELECT id, org_id, project_id, provider, repo_url, branch, webhook_secret,
                last_deployed_sha, service_id, created_at
         FROM edge_function_groups WHERE org_id = $1 ORDER BY created_at DESC"
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(Json(ApiResponse::ok(groups)))
}

async fn create_group(
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    auth: AuthUser,
    headers: HeaderMap,
    Json(body): Json<CreateFunctionGroupRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    if !state.config.edge_functions.enabled {
        return Err(AppError::BadRequest("edge functions are not enabled".into()).into());
    }

    let secret = Uuid::new_v4().to_string();
    let group_id = Uuid::new_v4();

    // When a project is specified, create a synthetic service row so the group
    // participates in topology, domains, and all shared service infrastructure.
    let service_id: Option<Uuid> = if let Some(project_id) = body.project_id {
        let sid = Uuid::now_v7();
        let repo_name = body.repo_url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("edge-fn")
            .trim_end_matches(".git")
            .to_string();
        let slug = format!("efg-{}", &group_id.to_string()[..8]);
        sqlx::query(
            "INSERT INTO services (id, project_id, name, slug, type, status, replicas, ports)
             VALUES ($1, $2, $3, $4, 'edge_functions', 'running', 1, '[]'::jsonb)",
        )
        .bind(sid)
        .bind(project_id)
        .bind(&repo_name)
        .bind(&slug)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        Some(sid)
    } else {
        None
    };

    let group: EdgeFunctionGroup = sqlx::query_as(
        "INSERT INTO edge_function_groups
             (id, org_id, project_id, provider, repo_url, branch, webhook_secret, service_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, org_id, project_id, provider, repo_url, branch, webhook_secret,
                   last_deployed_sha, service_id, created_at",
    )
    .bind(group_id)
    .bind(org_id)
    .bind(body.project_id)
    .bind(&body.provider)
    .bind(&body.repo_url)
    .bind(&body.branch)
    .bind(&secret)
    .bind(service_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    // Auto-register webhook on the git provider if a git_provider_id was supplied.
    let mut webhook_registered = false;
    let mut webhook_error: Option<String> = None;

    if let Some(git_provider_id) = body.git_provider_id {
        match register_webhook_on_provider(&state, &headers, &group, git_provider_id).await {
            Ok(_) => webhook_registered = true,
            Err(e) => {
                tracing::warn!(group_id = %group.id, "webhook auto-registration failed: {e}");
                webhook_error = Some(e);
            }
        }
    }

    // Kick off the initial deploy immediately (best-effort).
    let state_clone = state.clone();
    let group_clone = group.clone();
    let user_id = auth.user_id;
    tokio::spawn(async move {
        if let Err(e) = manager::deploy_from_git(&state_clone, &group_clone, "initial", Some(user_id)).await {
            tracing::warn!(group_id = %group_clone.id, "initial edge function deploy failed: {e}");
        }
    });

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "group": group,
        "webhook_registered": webhook_registered,
        "webhook_error": webhook_error,
    }))))
}

async fn register_webhook_on_provider(
    state: &AppState,
    headers: &HeaderMap,
    group: &EdgeFunctionGroup,
    git_provider_id: Uuid,
) -> Result<(), String> {
    #[derive(sqlx::FromRow)]
    struct GitProvider {
        provider_type: String,
        token: String,
    }

    let provider: GitProvider = sqlx::query_as(
        "SELECT provider_type, token FROM git_providers WHERE id = $1"
    )
    .bind(git_provider_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "git provider not found".to_string())?;

    let base_url = resolve_base_url(&state.config, headers);
    if base_url.contains("://localhost") || base_url.contains("://127.0.0.1") {
        return Err("localhost URLs cannot receive webhooks from GitHub/GitLab".into());
    }

    let webhook_url = format!(
        "{}/api/webhooks/{}/fn/{}/{}",
        base_url, group.provider, group.id, group.webhook_secret
    );

    match provider.provider_type.as_str() {
        "github" => {
            let (owner, repo) = parse_github_repo(&group.repo_url)
                .ok_or_else(|| "could not parse owner/repo from URL".to_string())?;

            let res = state.http_client
                .post(format!("https://api.github.com/repos/{owner}/{repo}/hooks"))
                .header("User-Agent", "shipyard-api")
                .header("Accept", "application/vnd.github+json")
                .header("Authorization", format!("token {}", provider.token))
                .json(&serde_json::json!({
                    "name": "web",
                    "active": true,
                    "events": ["push"],
                    "config": {
                        "url": webhook_url,
                        "content_type": "json",
                        "secret": group.webhook_secret,
                        "insecure_ssl": "0"
                    }
                }))
                .send()
                .await
                .map_err(|e| format!("GitHub request failed: {e}"))?;

            let status = res.status();
            if status.is_success() || status == StatusCode::UNPROCESSABLE_ENTITY {
                Ok(())
            } else {
                let body = res.text().await.unwrap_or_default();
                Err(format!("GitHub returned {status}: {body}"))
            }
        }
        "gitlab" => {
            let project_path = parse_gitlab_project(&group.repo_url)
                .ok_or_else(|| "could not parse GitLab project path".to_string())?;
            let encoded = urlencoding::encode(&project_path);

            let res = state.http_client
                .post(format!("https://gitlab.com/api/v4/projects/{encoded}/hooks"))
                .header("Authorization", format!("Bearer {}", provider.token))
                .json(&serde_json::json!({
                    "url": webhook_url,
                    "push_events": true,
                    "token": group.webhook_secret
                }))
                .send()
                .await
                .map_err(|e| format!("GitLab request failed: {e}"))?;

            let status = res.status();
            if status.is_success() { Ok(()) }
            else {
                let body = res.text().await.unwrap_or_default();
                Err(format!("GitLab returned {status}: {body}"))
            }
        }
        other => Err(format!("provider '{other}' does not support auto-registration yet")),
    }
}

fn parse_github_repo(url: &str) -> Option<(String, String)> {
    let clean = url.trim_end_matches(".git");
    if clean.starts_with("git@") {
        let parts: Vec<&str> = clean.splitn(2, ':').collect();
        let path_parts: Vec<&str> = parts.get(1)?.split('/').collect();
        if path_parts.len() == 2 {
            return Some((path_parts[0].to_string(), path_parts[1].to_string()));
        }
    } else {
        let parts: Vec<&str> = clean.split('/').collect();
        if parts.len() >= 5 {
            return Some((
                parts[parts.len() - 2].to_string(),
                parts[parts.len() - 1].to_string(),
            ));
        }
    }
    None
}

fn parse_gitlab_project(url: &str) -> Option<String> {
    let clean = url.trim_end_matches(".git");
    if clean.starts_with("git@") {
        let parts: Vec<&str> = clean.splitn(2, ':').collect();
        return Some(parts.get(1)?.to_string());
    }
    if let Some(pos) = clean.find("gitlab.com/") {
        return Some(clean[pos + 11..].to_string());
    }
    None
}

fn resolve_base_url(config: &shipyard_common::config::AppConfig, headers: &HeaderMap) -> String {
    let configured = config.app_url.trim_end_matches('/');
    let is_local = configured.is_empty()
        || configured.contains("://localhost")
        || configured.contains("://127.0.0.1");

    if !is_local {
        return configured.to_string();
    }

    if let Some(host) = headers.get("x-forwarded-host").and_then(|v| v.to_str().ok()) {
        let proto = headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("https");
        return format!("{proto}://{host}");
    }

    configured.to_string()
}

async fn delete_group(
    State(state): State<AppState>,
    Path((org_id, group_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    // If the group has a synthetic service row, delete it — the service's
    // ON DELETE CASCADE takes care of the group row and all its domains.
    // Otherwise delete the group directly (org-level groups without a project).
    let service_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT service_id FROM edge_function_groups WHERE id = $1 AND org_id = $2",
    )
    .bind(group_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .flatten();

    if let Some(sid) = service_id {
        sqlx::query("DELETE FROM services WHERE id = $1")
            .bind(sid)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
    } else {
        sqlx::query("DELETE FROM edge_function_groups WHERE id = $1 AND org_id = $2")
            .bind(group_id)
            .bind(org_id)
            .execute(&state.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}

async fn trigger_group_deploy(
    State(state): State<AppState>,
    Path((org_id, group_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<DeployReport>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let group: EdgeFunctionGroup = sqlx::query_as(
        "SELECT id, org_id, project_id, provider, repo_url, branch, webhook_secret,
                last_deployed_sha, service_id, created_at
         FROM edge_function_groups WHERE id = $1 AND org_id = $2"
    )
    .bind(group_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or_else(|| AppError::NotFound("group not found".into()))?;

    let report = manager::deploy_from_git(&state, &group, "unknown", Some(auth.user_id)).await?;
    Ok(Json(ApiResponse::ok(report)))
}

// ─── Internal: manifest ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ManifestQuery {
    org_id: Uuid,
}

async fn runtime_manifest(
    State(state): State<AppState>,
    Query(q): Query<ManifestQuery>,
    headers: HeaderMap,
) -> Result<Json<Vec<FunctionManifestEntry>>, ApiAppError> {
    verify_runtime_secret(&state, &headers)?;
    let manifest = manager::build_manifest(&state, q.org_id).await?;
    Ok(Json(manifest))
}

// ─── Internal: log ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LogEntry {
    org_id: Uuid,
    fn_name: String,
    method: String,
    path: String,
    status_code: i32,
    duration_ms: i32,
    error: Option<String>,
}

async fn runtime_log(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LogEntry>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    verify_runtime_secret(&state, &headers)?;

    let fn_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM edge_functions WHERE org_id = $1 AND name = $2")
            .bind(body.org_id)
            .bind(&body.fn_name)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;

    if let Some(fn_id) = fn_id {
        sqlx::query(
            "INSERT INTO edge_function_invocation_logs
             (id, function_id, request_id, method, path, status_code, duration_ms, error)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(Uuid::new_v4())
        .bind(fn_id)
        .bind(Uuid::new_v4().to_string())
        .bind(&body.method)
        .bind(&body.path)
        .bind(body.status_code)
        .bind(body.duration_ms)
        .bind(&body.error)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "ok": true }))))
}

fn verify_runtime_secret(state: &AppState, headers: &HeaderMap) -> Result<(), ApiAppError> {
    let expected = state
        .config
        .edge_functions
        .runtime_secret
        .as_deref()
        .unwrap_or("");

    let provided = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");

    if expected.is_empty() || provided != expected {
        return Err(AppError::Unauthorized("invalid runtime secret".into()).into());
    }
    Ok(())
}

// ─── Domain management ────────────────────────────────────────────────────────

fn is_convenience_domain(hostname: &str) -> bool {
    hostname.ends_with(".nip.io") || hostname.ends_with(".traefik.me")
}

fn hostname_to_router_name(hostname: &str) -> String {
    hostname
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Writes a Traefik file-provider config for an edge function group's domains.
/// Routes all hostnames to the org's edge runtime Swarm service on port 8000.
async fn sync_edge_traefik_config(
    db: &sqlx::PgPool,
    group_id: Uuid,
    org_id: Uuid,
    entrypoint_http: &str,
    entrypoint_https: &str,
    dynamic_config_dir: Option<&str>,
) {
    let Some(dir) = dynamic_config_dir else { return };

    let short = &org_id.to_string()[..8];
    let upstream_host = format!("shipyard-edge-{short}");
    let path = std::path::Path::new(dir).join(format!("edge-{group_id}.yml"));

    let rows: Vec<(String, bool, String, Option<i32>)> = match sqlx::query_as(
        "SELECT hostname, tls_enabled, cert_provider, port
         FROM domains WHERE service_id = (
             SELECT service_id FROM edge_function_groups WHERE id = $1
         ) ORDER BY created_at ASC",
    )
    .bind(group_id)
    .fetch_all(db)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("sync_edge_traefik_config: DB error: {e}");
            return;
        }
    };

    if rows.is_empty() {
        let _ = tokio::fs::remove_file(&path).await;
        return;
    }

    let mut out = String::from("# Auto-generated by Shipyard edge functions — do not edit\n");
    out.push_str("http:\n");
    out.push_str("  routers:\n");

    for (hostname, tls_enabled, cert_provider, _port) in &rows {
        let rn = hostname_to_router_name(hostname);
        let convenience = is_convenience_domain(hostname);

        let _ = write!(out, "    {rn}-http:\n");
        let _ = write!(out, "      rule: \"Host(`{hostname}`)\"\n");
        let _ = write!(out, "      entryPoints:\n");
        let _ = write!(out, "        - {entrypoint_http}\n");
        let _ = write!(out, "      service: {rn}\n");

        let _ = write!(out, "    {rn}-https:\n");
        let _ = write!(out, "      rule: \"Host(`{hostname}`)\"\n");
        let _ = write!(out, "      entryPoints:\n");
        let _ = write!(out, "        - {entrypoint_https}\n");
        let _ = write!(out, "      middlewares:\n");
        let _ = write!(out, "        - shipyard-error-pages@file\n");
        if convenience || !*tls_enabled {
            let _ = write!(out, "      tls: {{}}\n");
        } else {
            let _ = write!(out, "      tls:\n");
            let _ = write!(out, "        certResolver: {cert_provider}\n");
        }
        let _ = write!(out, "      service: {rn}\n");
    }

    out.push_str("  services:\n");
    for (hostname, _, _, port) in &rows {
        let rn = hostname_to_router_name(hostname);
        let backend_port = port.unwrap_or(8000);
        let _ = write!(out, "    {rn}:\n");
        let _ = write!(out, "      loadBalancer:\n");
        let _ = write!(out, "        servers:\n");
        let _ = write!(out, "          - url: \"http://{upstream_host}:{backend_port}\"\n");
    }

    if let Err(e) = tokio::fs::create_dir_all(dir).await {
        tracing::warn!("sync_edge_traefik_config: could not create dir '{dir}': {e}");
        return;
    }
    if let Err(e) = tokio::fs::write(&path, out.as_bytes()).await {
        tracing::warn!("sync_edge_traefik_config: could not write '{path:?}': {e}");
    } else {
        tracing::info!("Wrote edge Traefik config: {path:?}");
    }
}

async fn list_group_domains(
    State(state): State<AppState>,
    Path((org_id, group_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<Vec<EdgeFunctionDomain>>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let domains: Vec<EdgeFunctionDomain> = sqlx::query_as(
        "SELECT id, service_id, hostname, tls_enabled, cert_provider, port,
                traefik_router_name, created_at
         FROM domains
         WHERE service_id = (
             SELECT service_id FROM edge_function_groups WHERE id = $1 AND org_id = $2
         )
         ORDER BY created_at ASC",
    )
    .bind(group_id)
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(Json(ApiResponse::ok(domains)))
}

async fn create_group_domain(
    State(state): State<AppState>,
    Path((org_id, group_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
    Json(body): Json<CreateEdgeFunctionDomainRequest>,
) -> Result<(StatusCode, Json<ApiResponse<EdgeFunctionDomain>>), ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    if body.hostname.is_empty() {
        return Err(AppError::BadRequest("hostname is required".into()).into());
    }

    // Resolve the group's service_id (implicitly verifies org ownership).
    let service_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT service_id FROM edge_function_groups WHERE id = $1 AND org_id = $2",
    )
    .bind(group_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .flatten();

    let service_id = service_id
        .ok_or_else(|| ApiAppError(AppError::NotFound("group not found or has no service".into())))?;

    let tls_enabled = body.tls_enabled && !is_convenience_domain(&body.hostname);
    let router_name = hostname_to_router_name(&body.hostname);

    let domain: EdgeFunctionDomain = sqlx::query_as(
        "INSERT INTO domains
             (id, service_id, hostname, tls_enabled, cert_provider, port, traefik_router_name)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, service_id, hostname, tls_enabled, cert_provider, port,
                   traefik_router_name, created_at",
    )
    .bind(Uuid::now_v7())
    .bind(service_id)
    .bind(&body.hostname)
    .bind(tls_enabled)
    .bind(&body.cert_provider)
    .bind(body.port)
    .bind(&router_name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiAppError(AppError::Conflict(format!("Domain '{}' already exists", body.hostname)))
        } else {
            ApiAppError(AppError::Database(msg))
        }
    })?;

    sync_edge_traefik_config(
        &state.db,
        group_id,
        org_id,
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
        state.config.traefik.dynamic_config_dir.as_deref(),
    ).await;

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(domain))))
}

async fn delete_group_domain(
    State(state): State<AppState>,
    Path((org_id, group_id, domain_id)): Path<(Uuid, Uuid, Uuid)>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let rows = sqlx::query(
        "DELETE FROM domains
         WHERE id = $1
           AND service_id = (
               SELECT service_id FROM edge_function_groups WHERE id = $2 AND org_id = $3
           )",
    )
    .bind(domain_id)
    .bind(group_id)
    .bind(org_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound("domain not found".into()).into());
    }

    sync_edge_traefik_config(
        &state.db,
        group_id,
        org_id,
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
        state.config.traefik.dynamic_config_dir.as_deref(),
    ).await;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}
