//! Orgs managed by an integration, addressed by the integrator's own id.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::put,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events::{self, Scope};

use super::{
    db_err, org_by_external, random_alnum, validate_external_id, IntegrationCaller, SCOPE_MANAGE,
};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/orgs/:external_id",
        put(upsert_org).get(get_org).delete(delete_org),
    )
}

#[derive(Deserialize)]
struct UpsertOrg {
    name: String,
    /// Plan name ('free', 'pro', 'max', or any enabled plan). Defaults to free
    /// on create; changes the plan when sent for an existing org.
    plan: Option<String>,
}

#[derive(Serialize)]
pub struct OrgOut {
    pub id: Uuid,
    pub external_id: String,
    pub name: String,
    pub slug: String,
    pub plan: Option<String>,
    pub created_at: DateTime<Utc>,
    /// True when this call created the org.
    pub created: bool,
}

/// Dashboard slug from a display name: lowercase, `a-z0-9-`, plus a random
/// suffix so two orgs named "Acme" don't collide.
pub fn org_slug(name: &str) -> String {
    let mut base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while base.contains("--") {
        base = base.replace("--", "-");
    }
    let base = base.trim_matches('-');
    let base = if base.is_empty() { "org" } else { base };
    let base: String = base.chars().take(40).collect();
    format!("{}-{}", base.trim_end_matches('-'), random_alnum(6))
}

async fn plan_by_name(db: &sqlx::PgPool, name: &str) -> Result<(Uuid, String), ApiAppError> {
    sqlx::query_as("SELECT id, name FROM plans WHERE name = $1 AND enabled = true LIMIT 1")
        .bind(name)
        .fetch_optional(db)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            ApiAppError(AppError::BadRequest(format!(
                "Unknown or disabled plan '{name}'"
            )))
        })
}

async fn load(db: &sqlx::PgPool, org_id: Uuid, created: bool) -> Result<OrgOut, ApiAppError> {
    let (id, external_id, name, slug, plan, created_at): (Uuid, Option<String>, String, String, Option<String>, DateTime<Utc>) =
        sqlx::query_as(
            "SELECT o.id, o.external_id, o.name, o.slug, p.name, o.created_at
             FROM organizations o LEFT JOIN org_billing b ON b.org_id = o.id LEFT JOIN plans p ON p.id = b.plan_id
             WHERE o.id = $1",
        )
        .bind(org_id)
        .fetch_one(db)
        .await
        .map_err(db_err)?;
    Ok(OrgOut {
        id,
        external_id: external_id.unwrap_or_default(),
        name,
        slug,
        plan,
        created_at,
        created,
    })
}

/// PUT /orgs/:external_id — create the org on first call, update its name
/// (and plan, if sent) afterwards. Safe to retry.
async fn upsert_org(
    caller: IntegrationCaller,
    Path(external_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpsertOrg>,
) -> Result<(StatusCode, Json<ApiResponse<OrgOut>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_external_id(&external_id)?;
    let name = body.name.trim();
    if name.is_empty() || name.len() > 100 {
        return Err(ApiAppError(AppError::BadRequest(
            "name must be 1-100 characters".into(),
        )));
    }

    if let Ok(org_id) = org_by_external(&state.db, &caller, &external_id).await {
        sqlx::query("UPDATE organizations SET name = $2 WHERE id = $1")
            .bind(org_id)
            .bind(name)
            .execute(&state.db)
            .await
            .map_err(db_err)?;
        if let Some(plan) = &body.plan {
            let (plan_id, plan_name) = plan_by_name(&state.db, plan).await?;
            let mut tx = state.db.begin().await.map_err(db_err)?;
            sqlx::query("UPDATE organizations SET plan_id = $2 WHERE id = $1")
                .bind(org_id)
                .bind(plan_id)
                .execute(&mut *tx)
                .await
                .map_err(db_err)?;
            crate::orgs::apply_plan(&mut tx, org_id, plan_id, &plan_name)
                .await
                .map_err(db_err)?;
            tx.commit().await.map_err(db_err)?;
        }
        return Ok((
            StatusCode::OK,
            Json(ApiResponse::ok(load(&state.db, org_id, false).await?)),
        ));
    }

    let (plan_id, plan_name) =
        plan_by_name(&state.db, body.plan.as_deref().unwrap_or("free")).await?;
    let org_id = Uuid::now_v7();
    let mut tx = state.db.begin().await.map_err(db_err)?;
    let inserted = sqlx::query(
        "INSERT INTO organizations (id, name, slug, plan_id, integration_id, external_id, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, NOW())
         ON CONFLICT (integration_id, external_id) WHERE integration_id IS NOT NULL DO NOTHING",
    )
    .bind(org_id)
    .bind(name)
    .bind(org_slug(name))
    .bind(plan_id)
    .bind(caller.integration_id)
    .bind(&external_id)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?
    .rows_affected();
    if inserted == 0 {
        // A concurrent call created it; return that one.
        tx.rollback().await.ok();
        let existing = org_by_external(&state.db, &caller, &external_id).await?;
        return Ok((
            StatusCode::OK,
            Json(ApiResponse::ok(load(&state.db, existing, false).await?)),
        ));
    }
    crate::orgs::apply_plan(&mut tx, org_id, plan_id, &plan_name)
        .await
        .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;

    events::emit(
        &state.db,
        Scope::org(org_id),
        "org.created",
        serde_json::json!({ "name": name, "plan": plan_name }),
    )
    .await;
    caller
        .audit(
            &state.db,
            "create_org",
            "organization",
            Some(org_id),
            serde_json::json!({ "external_id": external_id }),
        )
        .await;
    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(load(&state.db, org_id, true).await?)),
    ))
}

async fn get_org(
    caller: IntegrationCaller,
    Path(external_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let org_id = org_by_external(&state.db, &caller, &external_id).await?;
    let org = load(&state.db, org_id, false).await?;
    let apps: Vec<(String, String, Vec<String>)> = sqlx::query_as(
        "SELECT a.app_key, a.name, COALESCE(array_agg(p.environment ORDER BY p.environment) FILTER (WHERE p.id IS NOT NULL), '{}')
         FROM integration_apps a LEFT JOIN projects p ON p.app_id = a.id
         WHERE a.org_id = $1 GROUP BY a.id ORDER BY a.created_at",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(serde_json::json!({
        "org": org,
        "apps": apps.into_iter().map(|(key, name, envs)| serde_json::json!({ "key": key, "name": name, "environments": envs })).collect::<Vec<_>>(),
    }))))
}

/// DELETE /orgs/:external_id — terminate every app (workloads, data, dev
/// databases, domains, secrets), then delete the org.
async fn delete_org(
    caller: IntegrationCaller,
    Path(external_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let org_id = org_by_external(&state.db, &caller, &external_id).await?;

    let app_keys: Vec<String> =
        sqlx::query_scalar("SELECT app_key FROM integration_apps WHERE org_id = $1")
            .bind(org_id)
            .fetch_all(&state.db)
            .await
            .map_err(db_err)?;
    for key in &app_keys {
        let app = super::app_ref(&state.db, &caller, &external_id, key).await?;
        super::apps::terminate_app(&state, &app).await?;
    }
    // Projects created outside the integration (e.g. by a member in the dashboard).
    let others: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM projects WHERE org_id = $1")
        .bind(org_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;
    for project_id in &others {
        crate::projects::teardown::teardown_project(
            &state,
            *project_id,
            crate::projects::teardown::TeardownOptions {
                remove_volumes: true,
            },
        )
        .await?;
    }

    // Emit before the row goes: the event fan-out reads the org's integration.
    events::emit(
        &state.db,
        Scope::org(org_id),
        "org.deleted",
        serde_json::json!({ "apps_terminated": app_keys, "other_projects_removed": others.len() }),
    )
    .await;
    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(db_err)?;

    // The org's edge network (tenant isolation), once nothing uses it.
    let net = shipyard_docker::isolation::org_edge_network_name(
        &state.config.docker.label_prefix,
        org_id,
    );
    let docker = std::sync::Arc::clone(&state.docker);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        let _ = docker.remove_network(&net).await;
    });

    caller
        .audit(
            &state.db,
            "delete_org",
            "organization",
            Some(org_id),
            serde_json::json!({ "external_id": external_id }),
        )
        .await;
    Ok(Json(ApiResponse::ok(
        serde_json::json!({ "deleted": external_id, "apps_terminated": app_keys }),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_clean_and_unique() {
        let s = org_slug("Acme  Corp!");
        assert!(s.starts_with("acme-corp-"), "{s}");
        assert_eq!(s.len(), "acme-corp-".len() + 6);
        assert_ne!(org_slug("Acme"), org_slug("Acme"));
        assert!(org_slug("!!!").starts_with("org-"));
    }
}
