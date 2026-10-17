//! Moving an app, with all its environments, to another organization.
//!
//! The database move is one transaction: the app, its environments (projects),
//! secrets, dev database, publish history and registry namespaces change
//! owner together, so a failure leaves everything where it was. Container
//! services are then redeployed from the image they already run so they join
//! the new org's edge network (tenant isolation). Web addresses, volumes and
//! databases are untouched, so URLs keep working and no data is copied.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events::{self, Scope};

use super::{app_ref, db_err, org_by_external, random_alnum, validate_slug, IntegrationCaller, SCOPE_MANAGE};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/orgs/:org/apps/:app/transfer", post(transfer_app))
}

#[derive(Deserialize)]
struct TransferBody {
    /// The destination org (the id you use for it in `/orgs/{org}`).
    to_org: String,
    /// New app key in the destination, when the current one is taken there.
    app_key: Option<String>,
}

/// What `move_app` did, for the response and the events.
#[derive(Debug)]
pub struct Moved {
    pub app_key: String,
    /// `(project_id, environment, slug)` after the move.
    pub environments: Vec<(Uuid, Option<String>, String)>,
}

/// Why a move was refused (maps to 409 / 403 in the handler).
#[derive(Debug)]
pub enum MoveError {
    Conflict(String),
    Forbidden(String),
    Db(sqlx::Error),
}

impl From<sqlx::Error> for MoveError {
    fn from(e: sqlx::Error) -> Self {
        MoveError::Db(e)
    }
}

/// Moves `app_id` from `from_org` to `to_org` in one transaction.
pub async fn move_app(
    db: &sqlx::PgPool,
    app_id: Uuid,
    from_org: Uuid,
    to_org: Uuid,
    new_key: Option<&str>,
) -> Result<Moved, MoveError> {
    let mut tx = db.begin().await?;

    // Lock the app row so two transfers of the same app can't interleave.
    let (old_key,): (String,) =
        sqlx::query_as("SELECT app_key FROM integration_apps WHERE id = $1 AND org_id = $2 FOR UPDATE")
            .bind(app_id)
            .bind(from_org)
            .fetch_one(&mut *tx)
            .await?;
    let key = new_key.unwrap_or(&old_key).to_string();

    let key_taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM integration_apps WHERE org_id = $1 AND app_key = $2)",
    )
    .bind(to_org)
    .bind(&key)
    .fetch_one(&mut *tx)
    .await?;
    if key_taken {
        return Err(MoveError::Conflict(format!(
            "The destination org already has an app '{key}'. Send \"app_key\" with another name."
        )));
    }

    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM deployments d
             JOIN services s ON s.id = d.service_id
             JOIN projects p ON p.id = s.project_id
             WHERE p.app_id = $1 AND d.status::text IN ('running', 'pending', 'queued'))
         OR EXISTS (
             SELECT 1 FROM publishes pu
             JOIN projects p ON p.id = pu.project_id
             WHERE p.app_id = $1 AND pu.status = 'running')",
    )
    .bind(app_id)
    .fetch_one(&mut *tx)
    .await?;
    if busy {
        return Err(MoveError::Conflict(
            "A publish or deployment of this app is still running. Try again when it finishes.".into(),
        ));
    }

    let projects: Vec<(Uuid, String, Option<String>)> =
        sqlx::query_as("SELECT id, slug, environment FROM projects WHERE app_id = $1 ORDER BY created_at")
            .bind(app_id)
            .fetch_all(&mut *tx)
            .await?;

    // Same project quota as creating the environments there.
    let (count, limit): (i64, Option<i32>) = sqlx::query_as(
        "SELECT (SELECT COUNT(*) FROM projects WHERE org_id = $1),
                (SELECT max_projects FROM org_quota WHERE org_id = $1)",
    )
    .bind(to_org)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(max) = limit {
        if max != -1 && count + projects.len() as i64 > max as i64 {
            return Err(MoveError::Forbidden(format!(
                "Plan limit reached: the destination org allows {max} environment(s)/project(s) and has {count}; this app brings {}",
                projects.len()
            )));
        }
    }

    let mut environments = vec![];
    for (project_id, slug, environment) in &projects {
        // (org_id, slug) is unique; keep the slug unless the destination uses it.
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM projects WHERE org_id = $1 AND slug = $2)",
        )
        .bind(to_org)
        .bind(slug)
        .fetch_one(&mut *tx)
        .await?;
        let new_slug = if taken {
            format!("{}-{}", slug.chars().take(55).collect::<String>().trim_end_matches('-'), random_alnum(5))
        } else {
            slug.clone()
        };
        sqlx::query("UPDATE projects SET org_id = $1, slug = $2, updated_at = NOW() WHERE id = $3")
            .bind(to_org)
            .bind(&new_slug)
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        environments.push((*project_id, environment.clone(), new_slug));
    }

    sqlx::query("UPDATE integration_apps SET org_id = $1, app_key = $2 WHERE id = $3")
        .bind(to_org)
        .bind(&key)
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE secrets SET org_id = $1 WHERE app_id = $2")
        .bind(to_org)
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE dev_databases SET org_id = $1 WHERE app_id = $2")
        .bind(to_org)
        .bind(app_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE publishes SET org_id = $1 WHERE project_id IN (SELECT id FROM projects WHERE app_id = $2)",
    )
    .bind(to_org)
    .bind(app_id)
    .execute(&mut *tx)
    .await?;
    // Images stay under their original registry path; the namespace just changes owner.
    sqlx::query(
        "UPDATE registry_namespaces SET org_id = $1 WHERE project_id IN (SELECT id FROM projects WHERE app_id = $2)",
    )
    .bind(to_org)
    .bind(app_id)
    .execute(&mut *tx)
    .await?;
    // Per-project access granted to the old org's members doesn't carry over.
    sqlx::query(
        "DELETE FROM project_members WHERE project_id IN (SELECT id FROM projects WHERE app_id = $1)",
    )
    .bind(app_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Moved { app_key: key, environments })
}

/// POST /orgs/{org}/apps/{app}/transfer  `{ "to_org": "...", "app_key"?: "..." }`
async fn transfer_app(
    caller: IntegrationCaller,
    Path((org, app_key)): Path<(String, String)>,
    State(state): State<AppState>,
    Json(body): Json<TransferBody>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    // Both orgs must belong to the caller's integration.
    caller.integration_id()?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let to_org = org_by_external(&state.db, &caller, &body.to_org).await?;
    if to_org == app.org_id {
        return Err(ApiAppError(AppError::BadRequest(
            "The app is already in that organization".into(),
        )));
    }
    if let Some(k) = &body.app_key {
        validate_slug("app key", k)?;
    }
    crate::deployments::check_billing_for_org(&state.db, to_org).await?;

    let moved = move_app(&state.db, app.app_id, app.org_id, to_org, body.app_key.as_deref())
        .await
        .map_err(|e| match e {
            MoveError::Conflict(m) => ApiAppError(AppError::Conflict(m)),
            MoveError::Forbidden(m) => ApiAppError(AppError::Forbidden(m)),
            MoveError::Db(e) => db_err(e),
        })?;

    let data = serde_json::json!({
        "app": moved.app_key,
        "from_org": org, "to_org": body.to_org,
        "from_org_id": app.org_id, "to_org_id": to_org,
        "environments": moved.environments.iter().map(|(id, env, slug)| serde_json::json!({
            "environment": env, "project_id": id, "slug": slug,
        })).collect::<Vec<_>>(),
    });
    // Both orgs see the move in their event feeds.
    events::emit(&state.db, Scope::org(app.org_id), "app.transferred", data.clone()).await;
    events::emit(&state.db, Scope::org(to_org), "app.transferred", data.clone()).await;
    caller
        .audit(&state.db, "transfer_app", "app", Some(app.app_id), data.clone())
        .await;

    // Containers are still attached to the old org's edge network. Redeploy
    // the image each one runs now, in creation order (database first), so it
    // joins the new org's network.
    let services: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT s.id, s.slug, s.type::text FROM services s
         JOIN projects p ON p.id = s.project_id
         WHERE p.app_id = $1 AND s.service_parent_id IS NULL
         ORDER BY p.created_at, s.created_at",
    )
    .bind(app.app_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    let mut redeploy: Vec<(Uuid, Uuid, String)> = vec![];
    let mut skipped = vec![];
    let mut redeploying = vec![];
    for (service_id, slug, svc_type) in services {
        if matches!(svc_type.as_str(), "static" | "docker_compose" | "sandbox_app") {
            continue; // not on an org edge network
        }
        let image: Option<String> = sqlx::query_scalar(
            "SELECT deployed_image FROM deployments
             WHERE service_id = $1 AND status::text = 'success' AND deployed_image IS NOT NULL
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(service_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err)?;
        let Some(image) = image else {
            skipped.push(slug);
            continue;
        };
        let deployment_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
             VALUES ($1, $2, $3, 'transfer', 'running'::deployment_status, NOW())",
        )
        .bind(deployment_id)
        .bind(service_id)
        .bind(format!("transfer by {}", caller.key_name))
        .execute(&state.db)
        .await
        .map_err(db_err)?;
        redeploying.push(serde_json::json!({ "service": slug, "deployment_id": deployment_id }));
        redeploy.push((service_id, deployment_id, image));
    }
    if !redeploy.is_empty() {
        let engine = super::publish::engine(&state);
        let notify = std::sync::Arc::clone(&state.swarm_sync_trigger);
        let by = format!("transfer by {}", caller.key_name);
        tokio::spawn(async move {
            for (service_id, deployment_id, image) in redeploy {
                if let Err(e) = engine.rollback(deployment_id, service_id, &by, &image).await {
                    tracing::error!(%deployment_id, %service_id, "transfer redeploy: {e}");
                }
            }
            notify.notify_one();
        });
    }

    Ok((
        StatusCode::OK,
        Json(ApiResponse::ok(serde_json::json!({
            "app": moved.app_key,
            "from_org": org,
            "to_org": body.to_org,
            "environments": data["environments"],
            "redeploying": redeploying,
            "skipped": skipped,
        }))),
    ))
}
