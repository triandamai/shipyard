//! Apps (one per Nomi project) and their environments: create, inspect,
//! logs, usage, and termination.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use shipyard_common::crypto;
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events::{self, Scope};

use super::{
    app_ref, db_err, env_project, org_by_external, validate_slug, AppRef, IntegrationCaller,
    SCOPE_MANAGE,
};
use crate::error::ApiAppError;
use crate::projects::teardown::{teardown_project, TeardownOptions};
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/orgs/:org/apps/:app",
            put(upsert_app).get(get_app).delete(delete_app),
        )
        .route(
            "/orgs/:org/apps/:app/envs/:env",
            get(get_env).delete(delete_env),
        )
        .route(
            "/orgs/:org/apps/:app/envs/:env/deployments/:deployment_id",
            get(get_deployment),
        )
        .route(
            "/orgs/:org/apps/:app/envs/:env/services/:service/logs",
            get(service_logs),
        )
        .route("/orgs/:org/apps/:app/envs/:env/usage", get(env_usage))
}

#[derive(Deserialize)]
struct UpsertApp {
    name: String,
    /// Also create the app's dev database (same as PUT .../dev-database).
    #[serde(default)]
    dev_database: bool,
}

/// PUT /orgs/:org/apps/:app — create on first call, rename afterwards.
async fn upsert_app(
    caller: IntegrationCaller,
    Path((org, app_key)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(body): Json<UpsertApp>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_slug("app key", &app_key)?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(ApiAppError(AppError::BadRequest(
            "name must be 1-100 characters".into(),
        )));
    }
    let org_id = org_by_external(&state.db, &caller, &org).await?;
    let (app_id, created): (Uuid, bool) = sqlx::query_as(
        "INSERT INTO integration_apps (id, org_id, app_key, name) VALUES ($1, $2, $3, $4)
         ON CONFLICT (org_id, app_key) DO UPDATE SET name = EXCLUDED.name
         RETURNING id, (xmax = 0)",
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(&app_key)
    .bind(name)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;
    let app = AppRef {
        org_id,
        app_id,
        app_key: app_key.clone(),
    };

    if created {
        events::emit(
            &state.db,
            Scope::org(org_id),
            "app.created",
            serde_json::json!({ "app": app_key, "name": name }),
        )
        .await;
        caller
            .audit(
                &state.db,
                "create_app",
                "app",
                Some(app_id),
                serde_json::json!({ "app": app_key, "org_id": org_id }),
            )
            .await;
    }
    let dev_database = if body.dev_database {
        Some(super::devdb::ensure_dev_database(&state, &app).await?)
    } else {
        None
    };
    Ok((
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(ApiResponse::ok(serde_json::json!({
            "key": app_key, "name": name, "created": created, "dev_database": dev_database,
        }))),
    ))
}

async fn get_app(
    caller: IntegrationCaller,
    Path((org, app_key)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let envs: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT environment, id FROM projects WHERE app_id = $1 ORDER BY created_at",
    )
    .bind(app.app_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let mut out = vec![];
    for (env, project_id) in envs {
        out.push(env_summary(&state, &env, project_id).await?);
    }
    let has_dev_db: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM dev_databases WHERE app_id = $1)")
            .bind(app.app_id)
            .fetch_one(&state.db)
            .await
            .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(serde_json::json!({
        "key": app.app_key, "environments": out, "dev_database": has_dev_db,
    }))))
}

/// Status of an environment: services with their state, last deployment and
/// URLs, and the latest publish.
async fn env_summary(
    state: &AppState,
    environment: &str,
    project_id: Uuid,
) -> Result<serde_json::Value, ApiAppError> {
    #[allow(clippy::type_complexity)]
    let services: Vec<(Uuid, String, String, String, i32, Option<Uuid>, Option<String>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT s.id, s.slug, s.type::text, s.status, s.replicas, d.id, d.status::text, d.created_at
         FROM services s
         LEFT JOIN LATERAL (
             SELECT id, status, created_at FROM deployments WHERE service_id = s.id ORDER BY created_at DESC LIMIT 1
         ) d ON TRUE
         WHERE s.project_id = $1 AND s.service_parent_id IS NULL ORDER BY s.created_at",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let mut items = vec![];
    for (id, slug, svc_type, status, replicas, dep_id, dep_status, dep_at) in services {
        let urls: Vec<String> = sqlx::query_scalar(
            "SELECT 'https://' || hostname FROM domains WHERE service_id = $1 ORDER BY created_at",
        )
        .bind(id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;
        let running: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM containers WHERE service_id = $1 AND status = 'running'",
        )
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err)?;
        items.push(serde_json::json!({
            "name": slug, "service_id": id, "type": svc_type, "status": status,
            "replicas": replicas, "running": running, "urls": urls,
            "last_deployment": dep_id.map(|d| serde_json::json!({ "id": d, "status": dep_status, "created_at": dep_at })),
        }));
    }
    #[allow(clippy::type_complexity)]
    let publish: Option<(Uuid, String, Vec<String>, Option<String>, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, status, urls, error, created_at FROM publishes WHERE project_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    Ok(serde_json::json!({
        "environment": environment,
        "project_id": project_id,
        "services": items,
        "last_publish": publish.map(|(id, status, urls, error, at)| serde_json::json!({
            "id": id, "status": status, "urls": urls, "error": error, "created_at": at,
        })),
    }))
}

async fn get_env(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    Ok(Json(ApiResponse::ok(
        env_summary(&state, &env, project_id).await?,
    )))
}

/// DELETE /orgs/:org/apps/:app/envs/:env — remove one environment and its data.
async fn delete_env(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    events::emit(
        &state.db,
        Scope::project(app.org_id, project_id),
        "project.deleted",
        serde_json::json!({ "app": app.app_key, "environment": env }),
    )
    .await;
    let report = teardown_project(
        &state,
        project_id,
        TeardownOptions {
            remove_volumes: true,
        },
    )
    .await?;
    caller
        .audit(
            &state.db,
            "delete_environment",
            "project",
            Some(project_id),
            serde_json::json!({ "app": app.app_key, "environment": env }),
        )
        .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "deleted": env, "report": report }),
    )))
}

/// Remove everything an app has on Shipyard: every environment (workloads,
/// volumes, domains, DNS records, source repos), the dev database, secrets,
/// and the app record.
pub async fn terminate_app(
    state: &AppState,
    app: &AppRef,
) -> Result<serde_json::Value, ApiAppError> {
    let envs: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT environment, id FROM projects WHERE app_id = $1")
            .bind(app.app_id)
            .fetch_all(&state.db)
            .await
            .map_err(db_err)?;
    let mut removed = vec![];
    for (env, project_id) in &envs {
        teardown_project(
            state,
            *project_id,
            TeardownOptions {
                remove_volumes: true,
            },
        )
        .await?;
        removed.push(env.clone());
    }
    let dev_db = match super::devdb::drop_dev_database(state, app).await {
        Ok(dropped) => dropped,
        // No dev database server configured: there can't be a dev database.
        Err(ApiAppError(AppError::BadRequest(_))) => false,
        Err(e) => return Err(e),
    };
    // Emit before deleting the app row so the envelope still carries the app key.
    events::emit(
        &state.db,
        Scope::org(app.org_id),
        "app.terminated",
        serde_json::json!({ "app": app.app_key, "environments": removed, "dev_database_dropped": dev_db }),
    )
    .await;
    sqlx::query("DELETE FROM integration_apps WHERE id = $1")
        .bind(app.app_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;
    Ok(
        serde_json::json!({ "app": app.app_key, "environments": removed, "dev_database_dropped": dev_db }),
    )
}

/// DELETE /orgs/:org/apps/:app — terminate the app (Nomi project deleted).
async fn delete_app(
    caller: IntegrationCaller,
    Path((org, app_key)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let report = terminate_app(&state, &app).await?;
    caller
        .audit(
            &state.db,
            "terminate_app",
            "app",
            Some(app.app_id),
            report.clone(),
        )
        .await;
    Ok(Json(ApiResponse::ok(report)))
}

/// GET .../envs/:env/deployments/:id — status, steps and build log.
async fn get_deployment(
    caller: IntegrationCaller,
    Path((org, app_key, env, deployment_id)): Path<(String, String, String, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    #[allow(clippy::type_complexity)]
    let dep: Option<(
        Uuid,
        String,
        String,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT d.service_id, s.slug, d.status::text, d.created_at, d.finished_at, d.deployed_image
         FROM deployments d JOIN services s ON s.id = d.service_id
         WHERE d.id = $1 AND s.project_id = $2",
    )
    .bind(deployment_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    let (service_id, service, status, created_at, finished_at, image) =
        dep.ok_or_else(|| ApiAppError(AppError::NotFound("Deployment not found".into())))?;
    #[allow(clippy::type_complexity)]
    let steps: Vec<(String, String, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT name, status::text, started_at, finished_at FROM deployment_steps WHERE deployment_id = $1 ORDER BY order_index",
    )
    .bind(deployment_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let logs: Vec<(String, String, DateTime<Utc>)> = sqlx::query_as(
        "SELECT level, message, created_at FROM deployment_logs WHERE deployment_id = $1 ORDER BY created_at LIMIT 5000",
    )
    .bind(deployment_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let secrets = secret_values(&state, service_id).await;
    Ok(Json(ApiResponse::ok(serde_json::json!({
        "id": deployment_id, "service": service, "status": status, "image": image,
        "created_at": created_at, "finished_at": finished_at,
        "steps": steps.into_iter().map(|(name, status, started, finished)| serde_json::json!({
            "name": name, "status": status, "started_at": started, "finished_at": finished,
        })).collect::<Vec<_>>(),
        "logs": logs.into_iter().map(|(level, message, at)| serde_json::json!({
            "level": level, "message": mask(&message, &secrets), "at": at,
        })).collect::<Vec<_>>(),
    }))))
}

/// Decrypted values of a service's secret env vars, longest first, for masking.
async fn secret_values(state: &AppState, service_id: Uuid) -> Vec<String> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT value_encrypted FROM service_envs WHERE service_id = $1 AND is_secret",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let plain: Vec<String> = rows
        .iter()
        .map(|enc| crypto::decrypt_or_passthrough(&state.config.auth.secret_key, enc))
        .collect();
    secrets_to_mask(&plain)
}

/// Values to hide: each secret, plus the password inside connection-string
/// secrets (which can show up on its own in error messages). Longest first so
/// a URL is masked whole before its password is.
pub fn secrets_to_mask(values: &[String]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for v in values {
        if let Ok(url) = reqwest::Url::parse(v) {
            if let Some(pw) = url.password() {
                out.push(pw.to_string());
            }
        }
        out.push(v.clone());
    }
    out.retain(|v| v.len() >= 6);
    out.sort_by_key(|v| std::cmp::Reverse(v.len()));
    out.dedup();
    out
}

/// Replace secret values in a log line with `****`.
pub fn mask(line: &str, secrets: &[String]) -> String {
    let mut out = line.to_string();
    for s in secrets {
        if out.contains(s.as_str()) {
            out = out.replace(s.as_str(), "****");
        }
    }
    out
}

#[derive(Deserialize)]
struct LogsQuery {
    #[serde(default = "default_tail")]
    tail: u32,
    /// e.g. "10m", "1h", or an RFC 3339 time.
    since: Option<String>,
}

fn default_tail() -> u32 {
    200
}

/// GET .../envs/:env/services/:service/logs — recent runtime output of all
/// replicas, with secret env values masked.
async fn service_logs(
    caller: IntegrationCaller,
    Path((org, app_key, env, service)): Path<(String, String, String, String)>,
    Query(q): Query<LogsQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    let service_id: Uuid =
        sqlx::query_scalar("SELECT id FROM services WHERE project_id = $1 AND slug = $2")
            .bind(project_id)
            .bind(&service)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err)?
            .ok_or_else(|| {
                ApiAppError(AppError::NotFound(format!("Service '{service}' not found")))
            })?;
    if let Some(since) = &q.since {
        let ok = since.len() <= 40
            && since
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-:.+".contains(&b));
        if !ok {
            return Err(ApiAppError(AppError::BadRequest("invalid 'since'".into())));
        }
    }

    // `docker service logs` reads every replica, on any node, through the manager.
    let name = format!("{}-{}", state.config.docker.label_prefix, service_id);
    let mut cmd = tokio::process::Command::new("docker");
    cmd.args([
        "service",
        "logs",
        "--raw",
        "--timestamps",
        &format!("--tail={}", q.tail.clamp(1, 5000)),
    ]);
    if let Some(since) = &q.since {
        cmd.arg(format!("--since={since}"));
    }
    cmd.arg(&name);
    let output = cmd
        .output()
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("docker service logs: {e}"))))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        if err.contains("no such service") || err.contains("not found") {
            return Ok(Json(ApiResponse::ok(
                serde_json::json!({ "service": service, "lines": [], "note": "not running" }),
            )));
        }
        return Err(ApiAppError(AppError::Internal(format!(
            "docker service logs: {}",
            err.trim()
        ))));
    }
    let secrets = secret_values(&state, service_id).await;
    let mut lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .chain(String::from_utf8_lossy(&output.stderr).lines())
        .map(|l| mask(l, &secrets))
        .collect();
    // Interleaved stdout/stderr: order by the leading timestamp.
    lines.sort();
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "service": service, "lines": lines }),
    )))
}

/// GET .../envs/:env/usage — CPU, memory, network and disk I/O of each
/// running container on this node, plus configured limits.
async fn env_usage(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    let rows: Vec<(String, Option<f64>, Option<i64>, String)> = sqlx::query_as(
        "SELECT s.slug, s.cpu_limit, s.memory_limit_mb, c.docker_container_id
         FROM services s JOIN containers c ON c.service_id = s.id
         WHERE s.project_id = $1 AND c.status = 'running'",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let mut out = vec![];
    for (slug, cpu_limit, memory_limit_mb, container_id) in rows {
        let path = format!("/v1.45/containers/{container_id}/stats?stream=false");
        let stats = match shipyard_docker::raw_request("GET", &path, None).await {
            Ok(body) => serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .map(|v| {
                    serde_json::to_value(crate::shorthand::parse_stats_snapshot(&v))
                        .unwrap_or_default()
                }),
            // On another node: not visible from here.
            Err(_) => None,
        };
        out.push(serde_json::json!({
            "service": slug, "container_id": container_id, "stats": stats,
            "limits": { "cpu": cpu_limit, "memory_mb": memory_limit_mb },
        }));
    }
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "environment": env, "containers": out }),
    )))
}

#[cfg(test)]
mod tests {
    use super::mask;

    #[test]
    fn connection_string_passwords_are_masked_alone_too() {
        let secrets = super::secrets_to_mask(&["postgres://app:hunter2pw@db:5432/app".to_string()]);
        assert_eq!(
            mask(
                "auth failed for hunter2pw; url postgres://app:hunter2pw@db:5432/app",
                &secrets
            ),
            "auth failed for ****; url ****"
        );
    }

    #[test]
    fn masks_secret_values() {
        let secrets = vec!["supersecretvalue".to_string(), "abc123xyz".to_string()];
        assert_eq!(
            mask("connecting with supersecretvalue and abc123xyz", &secrets),
            "connecting with **** and ****"
        );
        assert_eq!(mask("nothing here", &secrets), "nothing here");
    }
}
