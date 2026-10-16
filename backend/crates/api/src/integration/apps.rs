//! Apps (one per Nomi project) and their environments: create, inspect,
//! logs, usage, and termination.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post, put},
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
            "/orgs/:org/apps/:app/envs/:env/deployments",
            get(list_deployments),
        )
        .route(
            "/orgs/:org/apps/:app/envs/:env/deployments/:deployment_id",
            get(get_deployment),
        )
        // Check a deployment knowing only its id (and org, or not even that).
        .route("/orgs/:org/deployments/:deployment_id", get(get_org_deployment))
        .route("/deployments/:deployment_id", get(get_any_deployment))
        .route(
            "/orgs/:org/apps/:app/envs/:env/services/:service/rollback",
            post(rollback_service),
        )
        .route("/orgs/:org/apps/:app/envs/:env/restart", post(restart_env))
        .route(
            "/orgs/:org/apps/:app/envs/:env/services/:service/env",
            get(list_service_env),
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
        // Live means the last deployment succeeded and something is serving:
        // running containers, or the shared nginx for a static site.
        let live = is_live(
            dep_status.as_deref(),
            svc_type == "static",
            running,
        );
        items.push(serde_json::json!({
            "name": slug, "service_id": id, "type": svc_type, "status": status,
            "replicas": replicas, "running": running, "urls": urls, "live": live,
            "last_deployment": dep_id.map(|d| serde_json::json!({
                "id": d,
                "status": dep_status.as_deref().map(super::normalize_deploy_status),
                "raw_status": dep_status,
                "created_at": dep_at,
            })),
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

/// A service is live when its latest deployment succeeded and it is serving
/// traffic: at least one running container, or any static site (served by the
/// shared nginx, with no container of its own).
pub fn is_live(last_deployment_raw: Option<&str>, is_static: bool, running_containers: i64) -> bool {
    matches!(last_deployment_raw, Some("success") | Some("succeeded"))
        && (is_static || running_containers > 0)
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

#[derive(Deserialize)]
struct DeploymentQuery {
    /// `false` returns status and steps only. Default `true`.
    logs: Option<bool>,
    /// Only logs after this timestamp (the `next_after` of the previous call).
    after: Option<DateTime<Utc>>,
    /// Most log lines to return (default 2000, at most 5000).
    logs_limit: Option<i64>,
}

/// GET .../envs/:env/deployments/:id: status, steps, failure summary and
/// build log.
async fn get_deployment(
    caller: IntegrationCaller,
    Path((org, app_key, _env, deployment_id)): Path<(String, String, String, Uuid)>,
    Query(q): Query<DeploymentQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    // The environment in the path is for symmetry with the other routes; a
    // deployment is found by id within the org, so an agent that remembers
    // the wrong environment still gets its answer.
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    Ok(Json(ApiResponse::ok(
        deployment_json(&state, app.org_id, deployment_id, &q).await?,
    )))
}

/// GET /orgs/:org/deployments/:id: no app or environment needed.
async fn get_org_deployment(
    caller: IntegrationCaller,
    Path((org, deployment_id)): Path<(String, Uuid)>,
    Query(q): Query<DeploymentQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let org_id = org_by_external(&state.db, &caller, &org).await?;
    Ok(Json(ApiResponse::ok(
        deployment_json(&state, org_id, deployment_id, &q).await?,
    )))
}

/// GET /deployments/:id: just the id. The org is looked up from the
/// deployment, then checked against what the credential may reach (an
/// unreachable deployment is reported as not found).
async fn get_any_deployment(
    caller: IntegrationCaller,
    Path(deployment_id): Path<Uuid>,
    Query(q): Query<DeploymentQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let org_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT p.org_id FROM deployments d
         JOIN services s ON s.id = d.service_id JOIN projects p ON p.id = s.project_id
         WHERE d.id = $1",
    )
    .bind(deployment_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    let not_found = || ApiAppError(AppError::NotFound("Deployment not found".into()));
    let org_id = org_id.ok_or_else(not_found)?;
    if !super::can_access_org(&state.db, &caller, org_id).await? {
        return Err(not_found());
    }
    Ok(Json(ApiResponse::ok(
        deployment_json(&state, org_id, deployment_id, &q).await?,
    )))
}

/// The deployment view shared by every route above.
///
/// `status` is `running`, `succeeded` or `failed` (`raw_status` has Shipyard's
/// own value). `urls` is only filled once the deployment succeeded, so a
/// client never links to an app that would still answer 502. For a failed
/// deployment, `failure` names the step, the error and the last log lines, so
/// an agent can fix the code without fetching the whole log.
async fn deployment_json(
    state: &AppState,
    org_id: Uuid,
    deployment_id: Uuid,
    q: &DeploymentQuery,
) -> Result<serde_json::Value, ApiAppError> {
    #[allow(clippy::type_complexity)]
    let dep: Option<(
        Uuid,
        String,
        String,
        DateTime<Utc>,
        Option<DateTime<Utc>>,
        Option<String>,
        String,
        String,
        Uuid,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT d.service_id, s.slug, d.status::text, d.created_at, d.finished_at, d.deployed_image,
                d.triggered_by, d.source_ref, p.id, p.environment, a.app_key
         FROM deployments d
         JOIN services s ON s.id = d.service_id
         JOIN projects p ON p.id = s.project_id
         LEFT JOIN integration_apps a ON a.id = p.app_id
         WHERE d.id = $1 AND p.org_id = $2",
    )
    .bind(deployment_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?;
    let (
        service_id,
        service,
        raw_status,
        created_at,
        finished_at,
        image,
        triggered_by,
        source_ref,
        project_id,
        environment,
        app_key,
    ) = dep.ok_or_else(|| ApiAppError(AppError::NotFound("Deployment not found".into())))?;
    let status = super::normalize_deploy_status(&raw_status);
    let status_name = status;

    #[allow(clippy::type_complexity)]
    let steps: Vec<(String, String, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT name, status::text, started_at, finished_at FROM deployment_steps
         WHERE deployment_id = $1 ORDER BY order_index",
    )
    .bind(deployment_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let secrets = secret_values(state, service_id).await;

    let include_logs = q.logs.unwrap_or(true);
    let logs: Vec<(String, String, DateTime<Utc>)> = if include_logs {
        sqlx::query_as(
            "SELECT level::text, message, timestamp FROM deployment_logs
             WHERE deployment_id = $1 AND ($2::timestamptz IS NULL OR timestamp > $2)
             ORDER BY timestamp LIMIT $3",
        )
        .bind(deployment_id)
        .bind(q.after)
        .bind(q.logs_limit.unwrap_or(2000).clamp(1, 5000))
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
    } else {
        vec![]
    };
    let next_after = logs.last().map(|l| l.2).or(q.after);

    let failure = if status == "failed" {
        let step: Option<String> = sqlx::query_scalar(
            "SELECT name FROM deployment_steps WHERE deployment_id = $1 AND status::text = 'failed'
             ORDER BY order_index LIMIT 1",
        )
        .bind(deployment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;
        let message: Option<String> = sqlx::query_scalar(
            "SELECT message FROM deployment_logs WHERE deployment_id = $1 AND level::text = 'error'
             ORDER BY timestamp DESC LIMIT 1",
        )
        .bind(deployment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;
        let mut tail: Vec<String> = sqlx::query_scalar(
            "SELECT message FROM deployment_logs WHERE deployment_id = $1
             ORDER BY timestamp DESC LIMIT 40",
        )
        .bind(deployment_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;
        tail.reverse();
        Some(serde_json::json!({
            "step": step,
            "phase": step.as_deref().map(super::step_phase),
            "message": message.map(|m| mask(&m, &secrets)),
            "tail": tail.iter().map(|l| mask(l, &secrets)).collect::<Vec<_>>(),
        }))
    } else {
        None
    };

    let urls: Vec<String> = if status == "succeeded" {
        sqlx::query_scalar(
            "SELECT CASE WHEN tls_enabled THEN 'https://' ELSE 'http://' END || hostname
             FROM domains WHERE service_id = $1 ORDER BY created_at",
        )
        .bind(service_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?
    } else {
        vec![]
    };

    Ok(serde_json::json!({
        "id": deployment_id,
        "app": app_key,
        "environment": environment,
        "project_id": project_id,
        "service": service,
        "service_id": service_id,
        "status": status,
        "raw_status": raw_status,
        "image": image,
        "triggered_by": triggered_by,
        "source_ref": source_ref,
        "urls": urls,
        "created_at": created_at,
        "finished_at": finished_at,
        "failure": failure,
        "steps": steps.into_iter().map(|(name, status, started, finished)| {
            serde_json::json!({
                "name": name,
                "phase": super::step_phase(&name),
                "status": super::normalize_step_status(&status, status_name),
                "started_at": started,
                "finished_at": finished,
            })
        }).collect::<Vec<_>>(),
        "logs": logs.into_iter().map(|(level, message, at)| serde_json::json!({
            "level": level,
            "message": mask(&message, &secrets),
            "at": at,
        })).collect::<Vec<_>>(),
        "next_after": next_after,
    }))
}

#[derive(Deserialize)]
struct ListDeploymentsQuery {
    service: Option<String>,
    limit: Option<i64>,
}

/// GET .../envs/:env/deployments: recent deployments, newest first.
async fn list_deployments(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    Query(q): Query<ListDeploymentsQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    #[allow(clippy::type_complexity)]
    let rows: Vec<(Uuid, String, String, String, String, Option<String>, DateTime<Utc>, Option<DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT d.id, s.slug, d.status::text, d.triggered_by, d.source_ref, d.deployed_image,
                    d.created_at, d.finished_at
             FROM deployments d JOIN services s ON s.id = d.service_id
             WHERE s.project_id = $1 AND ($2::text IS NULL OR s.slug = $2)
             ORDER BY d.created_at DESC LIMIT $3",
        )
        .bind(project_id)
        .bind(&q.service)
        .bind(q.limit.unwrap_or(20).clamp(1, 100))
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(
        rows.into_iter()
            .map(|(id, service, raw, by, source_ref, image, created_at, finished_at)| {
                serde_json::json!({
                    "id": id,
                    "service": service,
                    "status": super::normalize_deploy_status(&raw),
                    "raw_status": raw,
                    "triggered_by": by,
                    "source_ref": source_ref,
                    "image": image,
                    "created_at": created_at,
                    "finished_at": finished_at,
                })
            })
            .collect(),
    )))
}

#[derive(Deserialize, Default)]
struct RollbackBody {
    /// Deployment to return to. Default: the last successful deployment
    /// before the current one.
    deployment_id: Option<Uuid>,
}

/// The image a rollback should return to: the requested deployment's, or the
/// second most recent successful one (the most recent is what's running).
pub fn pick_rollback_target(
    successes_newest_first: &[(Uuid, String)],
    requested: Option<Uuid>,
) -> Result<(Uuid, String), String> {
    match requested {
        Some(id) => successes_newest_first
            .iter()
            .find(|(d, _)| *d == id)
            .cloned()
            .ok_or_else(|| {
                "That deployment didn't succeed, has no recorded image, or belongs to another service".to_string()
            }),
        None => successes_newest_first
            .get(1)
            .cloned()
            .ok_or_else(|| "There is no earlier successful deployment to return to".to_string()),
    }
}

/// POST .../envs/:env/services/:service/rollback: redeploy the image of an
/// earlier successful deployment. No rebuild, so it takes seconds. Database
/// migrations are not undone.
async fn rollback_service(
    caller: IntegrationCaller,
    Path((org, app_key, env, service)): Path<(String, String, String, String)>,
    State(state): State<AppState>,
    body: Option<Json<RollbackBody>>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let requested = body.and_then(|b| b.0.deployment_id);
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    crate::deployments::check_billing_for_org(&state.db, app.org_id).await?;

    let (service_id, svc_type): (Uuid, String) = sqlx::query_as(
        "SELECT id, type::text FROM services WHERE project_id = $1 AND slug = $2",
    )
    .bind(project_id)
    .bind(&service)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err)?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service '{service}' not found"))))?;
    if matches!(svc_type.as_str(), "static" | "docker_compose" | "sandbox_app") {
        return Err(ApiAppError(AppError::BadRequest(
            "Only container services can roll back; publish the earlier source again instead".into(),
        )));
    }

    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM deployments WHERE service_id = $1 AND status::text IN ('running', 'pending', 'queued'))",
    )
    .bind(service_id)
    .fetch_one(&state.db)
    .await
    .map_err(db_err)?;
    if busy {
        return Err(ApiAppError(AppError::Conflict(
            "A deployment of this service is still in progress".into(),
        )));
    }

    let successes: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, deployed_image FROM deployments
         WHERE service_id = $1 AND status::text = 'success' AND deployed_image IS NOT NULL
         ORDER BY created_at DESC LIMIT 20",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let (target_id, image) = pick_rollback_target(&successes, requested)
        .map_err(|m| ApiAppError(AppError::Conflict(m)))?;

    let deployment_id = Uuid::now_v7();
    let triggered_by = format!("rollback by {}", caller.key_name);
    sqlx::query(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         VALUES ($1, $2, $3, $4, 'running'::deployment_status, NOW())",
    )
    .bind(deployment_id)
    .bind(service_id)
    .bind(&triggered_by)
    .bind(format!("rollback:{target_id}"))
    .execute(&state.db)
    .await
    .map_err(db_err)?;

    let engine = super::publish::engine(&state);
    let notify = std::sync::Arc::clone(&state.swarm_sync_trigger);
    let (img, by) = (image.clone(), triggered_by.clone());
    tokio::spawn(async move {
        if let Err(e) = engine.rollback(deployment_id, service_id, &by, &img).await {
            tracing::error!(%deployment_id, "integration rollback: {e}");
        }
        notify.notify_one();
    });

    caller
        .audit(
            &state.db,
            "rollback",
            "deployment",
            Some(deployment_id),
            serde_json::json!({ "service_id": service_id, "to": target_id, "image": image }),
        )
        .await;
    Ok((
        StatusCode::ACCEPTED,
        Json(ApiResponse::ok(serde_json::json!({
            "deployment_id": deployment_id,
            "rolled_back_to": target_id,
            "image": image,
            "status": "running",
        }))),
    ))
}

#[derive(Deserialize, Default)]
struct RestartBody {
    /// Restart only this service. Default: every container service.
    service: Option<String>,
}

/// POST .../envs/:env/restart: apply the current env vars and secrets to the
/// running services, replacing their tasks, with no rebuild. Use after
/// changing a secret.
async fn restart_env(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    State(state): State<AppState>,
    body: Option<Json<RestartBody>>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let only = body.and_then(|b| b.0.service);
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    crate::deployments::check_billing_for_org(&state.db, app.org_id).await?;

    let services: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, slug, type::text FROM services
         WHERE project_id = $1 AND service_parent_id IS NULL AND ($2::text IS NULL OR slug = $2)
         ORDER BY created_at",
    )
    .bind(project_id)
    .bind(&only)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    if services.is_empty() {
        return Err(ApiAppError(AppError::NotFound(match only {
            Some(s) => format!("Service '{s}' not found"),
            None => "This environment has no services".into(),
        })));
    }

    let mut restarted = vec![];
    let mut skipped = vec![];
    for (id, slug, svc_type) in services {
        if matches!(svc_type.as_str(), "static" | "docker_compose" | "sandbox_app") {
            skipped.push(serde_json::json!({ "service": slug, "reason": format!("{svc_type} services don't restart this way") }));
            continue;
        }
        match crate::shorthand::apply_current_env(&state, id).await {
            Ok(()) => restarted.push(slug),
            Err(e) => skipped.push(serde_json::json!({ "service": slug, "reason": e.0.to_string() })),
        }
    }
    caller
        .audit(
            &state.db,
            "restart_environment",
            "project",
            Some(project_id),
            serde_json::json!({ "app": app.app_key, "environment": env, "restarted": restarted }),
        )
        .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "restarted": restarted, "skipped": skipped }),
    )))
}

/// GET .../services/:service/env: which variables a service has and where
/// each comes from. Never values.
async fn list_service_env(
    caller: IntegrationCaller,
    Path((org, app_key, env, service)): Path<(String, String, String, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;
    let service_id: Uuid = sqlx::query_scalar("SELECT id FROM services WHERE project_id = $1 AND slug = $2")
        .bind(project_id)
        .bind(&service)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Service '{service}' not found"))))?;
    let rows: Vec<(String, bool, Option<String>)> = sqlx::query_as(
        "SELECT e.key, e.is_secret, s.ref
         FROM service_envs e
         LEFT JOIN secret_bindings b ON b.service_id = e.service_id AND b.env_key = e.key
         LEFT JOIN secrets s ON s.id = b.secret_id
         WHERE e.service_id = $1 AND e.key NOT LIKE '\\_\\_%' ESCAPE '\\'
         ORDER BY e.key",
    )
    .bind(service_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(
        rows.into_iter()
            .map(|(name, is_secret, secret_ref)| {
                serde_json::json!({
                    "name": name,
                    "secret": is_secret,
                    // Set from a secret: change it with PUT .../secrets/{name}, then restart.
                    "secret_ref": secret_ref,
                })
            })
            .collect(),
    )))
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
    use uuid::Uuid;
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
    fn live_needs_a_successful_deploy_and_something_serving() {
        assert!(super::is_live(Some("success"), false, 1));
        assert!(!super::is_live(Some("success"), false, 0), "deployed but no container yet");
        assert!(!super::is_live(Some("running"), false, 1), "mid-deploy");
        assert!(!super::is_live(Some("failed"), false, 1));
        assert!(!super::is_live(None, false, 1));
        assert!(super::is_live(Some("success"), true, 0), "static sites have no container");
    }

    #[test]
    fn rollback_picks_the_previous_success() {
        let (a, b, c) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let list = vec![(a, "img:3".to_string()), (b, "img:2".to_string()), (c, "img:1".to_string())];
        assert_eq!(super::pick_rollback_target(&list, None).unwrap().0, b);
        assert_eq!(super::pick_rollback_target(&list, Some(c)).unwrap().1, "img:1");
        assert!(super::pick_rollback_target(&list, Some(Uuid::now_v7())).is_err());
        assert!(super::pick_rollback_target(&list[..1], None).is_err(), "only one version exists");
        assert!(super::pick_rollback_target(&[], None).is_err());
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
