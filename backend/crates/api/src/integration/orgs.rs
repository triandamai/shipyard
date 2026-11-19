//! Orgs managed by an integration, addressed by the integrator's own id.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::events::{self, Scope};
use shipyard_db::labels::{self, Labels, Table};

use super::{
    can_delete_org, db_err, org_by_external, random_alnum, validate_external_id, Access,
    IntegrationCaller, SCOPE_MANAGE,
};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/plans", get(list_plans))
        .route(
            "/orgs/:external_id",
            put(upsert_org)
                .patch(upsert_org)
                .get(get_org)
                .delete(delete_org),
        )
        .route(
            "/orgs/:external_id/quota",
            get(get_org_quota)
                .put(update_org_quota)
                .patch(update_org_quota),
        )
        .route(
            "/orgs/:external_id/plan",
            get(get_org_quota)
                .put(update_org_quota)
                .patch(update_org_quota),
        )
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IntegrationPlanOut {
    pub id: Uuid,
    pub name: String,
    pub cpu_cores: i32,
    pub memory_mb: i32,
    pub max_replicas: i32,
    pub node_count: i32,
    pub max_members: i32,
    pub max_projects: i32,
    pub max_orgs: i32,
    pub max_parallel_deployments: i32,
    pub max_git_providers: i32,
    pub price_monthly: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaLimits {
    pub max_projects: i32,
    pub max_members: i32,
    pub max_replicas: i32,
    pub max_parallel_deployments: i32,
    pub max_git_providers: i32,
    pub max_orgs: i32,
    pub node_count: i32,
    pub cpu_cores: i32,
    pub memory_mb: i32,
}

impl Default for QuotaLimits {
    fn default() -> Self {
        Self {
            max_projects: 3,
            max_members: 5,
            max_replicas: 1,
            max_parallel_deployments: 1,
            max_git_providers: 1,
            max_orgs: 1,
            node_count: 1,
            cpu_cores: 1,
            memory_mb: 2048,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_projects: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_members: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_replicas: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_parallel_deployments: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_git_providers: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_orgs: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_cores: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<i32>,
}

impl QuotaPatch {
    pub fn is_empty(&self) -> bool {
        self.max_projects.is_none()
            && self.max_members.is_none()
            && self.max_replicas.is_none()
            && self.max_parallel_deployments.is_none()
            && self.max_git_providers.is_none()
            && self.max_orgs.is_none()
            && self.node_count.is_none()
            && self.cpu_cores.is_none()
            && self.memory_mb.is_none()
    }

    pub fn merged_with(&self, nested: Option<&QuotaPatch>) -> Self {
        let Some(n) = nested else {
            return self.clone();
        };
        Self {
            max_projects: n.max_projects.or(self.max_projects),
            max_members: n.max_members.or(self.max_members),
            max_replicas: n.max_replicas.or(self.max_replicas),
            max_parallel_deployments: n.max_parallel_deployments.or(self.max_parallel_deployments),
            max_git_providers: n.max_git_providers.or(self.max_git_providers),
            max_orgs: n.max_orgs.or(self.max_orgs),
            node_count: n.node_count.or(self.node_count),
            cpu_cores: n.cpu_cores.or(self.cpu_cores),
            memory_mb: n.memory_mb.or(self.memory_mb),
        }
    }

    pub fn validate(&self) -> Result<(), ApiAppError> {
        let fields = [
            ("max_projects", self.max_projects),
            ("max_members", self.max_members),
            ("max_replicas", self.max_replicas),
            ("max_parallel_deployments", self.max_parallel_deployments),
            ("max_git_providers", self.max_git_providers),
            ("max_orgs", self.max_orgs),
            ("node_count", self.node_count),
            ("cpu_cores", self.cpu_cores),
            ("memory_mb", self.memory_mb),
        ];
        for (name, val) in fields {
            if let Some(v) = val {
                if v < -1 || v == 0 {
                    return Err(ApiAppError(AppError::BadRequest(format!(
                        "{name} must be -1 (unlimited) or >= 1, got {v}"
                    ))));
                }
            }
        }
        Ok(())
    }
}

pub fn validate_sub_status(raw: &str) -> Result<&'static str, ApiAppError> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "active" => Ok("active"),
        "past_due" => Ok("past_due"),
        "canceled" | "cancelled" => Ok("canceled"),
        "trialing" => Ok("trialing"),
        other => Err(ApiAppError(AppError::BadRequest(format!(
            "sub_status '{other}' must be one of: active, past_due, canceled, trialing"
        )))),
    }
}

#[derive(Default, Deserialize)]
struct UpsertOrg {
    /// Display name (required on create; optional when updating an existing org).
    name: Option<String>,
    /// Plan name ('free', 'pro', 'max', or any enabled plan name/UUID). Defaults to free
    /// on create; changes the plan when sent for an existing org.
    plan: Option<String>,
    /// Billing subscription status ('active', 'past_due', 'canceled', 'trialing').
    sub_status: Option<String>,
    /// Optional per-org quota overrides applied on top of the plan defaults.
    quota: Option<QuotaPatch>,
    /// Labels to set on the org; a null value removes that label.
    #[serde(default)]
    labels: BTreeMap<String, Option<String>>,
}

#[derive(Default, Deserialize)]
struct UpdateOrgPlanOrQuota {
    /// Optional plan name or UUID ('free', 'pro', 'max', etc.).
    plan: Option<String>,
    /// Optional subscription status ('active', 'past_due', 'canceled', 'trialing').
    sub_status: Option<String>,
    /// Optional nested quota object `{ "quota": { "max_projects": 20, ... } }`.
    quota: Option<QuotaPatch>,
    /// Top-level quota fields `{ "max_projects": 20, "cpu_cores": 4, ... }`.
    #[serde(flatten)]
    flat_quota: QuotaPatch,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrgQuotaOut {
    pub org_id: Uuid,
    pub external_id: String,
    pub plan: Option<String>,
    pub plan_id: Option<Uuid>,
    pub sub_status: String,
    pub max_projects: i32,
    pub max_members: i32,
    pub max_replicas: i32,
    pub max_parallel_deployments: i32,
    pub max_git_providers: i32,
    pub max_orgs: i32,
    pub node_count: i32,
    pub cpu_cores: i32,
    pub memory_mb: i32,
    pub projects_used: i64,
    pub apps_used: i64,
    pub services_used: i64,
    pub members_used: i64,
}

#[derive(Serialize)]
pub struct OrgOut {
    pub id: Uuid,
    pub external_id: String,
    pub name: String,
    pub slug: String,
    pub plan: Option<String>,
    pub sub_status: String,
    pub quota: QuotaLimits,
    pub created_at: DateTime<Utc>,
    /// Integration that manages the org (e.g. "nomi").
    pub integration: Option<String>,
    pub labels: Labels,
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

/// Whether a dashboard-token caller is a platform superadmin.
async fn is_superadmin(db: &sqlx::PgPool, caller: &IntegrationCaller) -> Result<bool, ApiAppError> {
    match &caller.access {
        Access::User { user_id } => sqlx::query_scalar::<_, bool>(
            "SELECT COALESCE((SELECT is_superadmin FROM users WHERE id = $1), FALSE)",
        )
        .bind(user_id)
        .fetch_one(db)
        .await
        .map_err(db_err),
        _ => Ok(false),
    }
}

async fn plan_by_name(db: &sqlx::PgPool, identifier: &str) -> Result<(Uuid, String), ApiAppError> {
    let trimmed = identifier.trim();
    let plan_uuid = Uuid::parse_str(trimmed).ok();
    sqlx::query_as(
        "SELECT id, name FROM plans
         WHERE (LOWER(name) = LOWER($1) OR id = $2) AND enabled = true
         LIMIT 1",
    )
    .bind(trimmed)
    .bind(plan_uuid)
    .fetch_optional(db)
    .await
    .map_err(db_err)?
    .ok_or_else(|| {
        ApiAppError(AppError::BadRequest(format!(
            "Unknown or disabled plan '{trimmed}'"
        )))
    })
}

async fn load_quota_limits(db: &sqlx::PgPool, org_id: Uuid) -> Result<QuotaLimits, ApiAppError> {
    let row: Option<(i32, i32, i32, i32, i32, i32, i32, i32, i32)> = sqlx::query_as(
        "SELECT max_projects, max_members, max_replicas, max_parallel_deployments,
                max_git_providers, max_orgs, node_count, cpu_cores, memory_mb
         FROM org_quota WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_optional(db)
    .await
    .map_err(db_err)?;

    Ok(match row {
        Some((
            max_projects,
            max_members,
            max_replicas,
            max_parallel_deployments,
            max_git_providers,
            max_orgs,
            node_count,
            cpu_cores,
            memory_mb,
        )) => QuotaLimits {
            max_projects,
            max_members,
            max_replicas,
            max_parallel_deployments,
            max_git_providers,
            max_orgs,
            node_count,
            cpu_cores,
            memory_mb,
        },
        None => QuotaLimits::default(),
    })
}

async fn load_quota_out(db: &sqlx::PgPool, org_id: Uuid) -> Result<OrgQuotaOut, ApiAppError> {
    #[allow(clippy::type_complexity)]
    let (external_id, plan, plan_id, sub_status): (
        Option<String>,
        Option<String>,
        Option<Uuid>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT o.external_id,
                COALESCE(p.name, op.name),
                COALESCE(q.plan_id, b.plan_id, o.plan_id),
                b.sub_status
         FROM organizations o
         LEFT JOIN org_billing b ON b.org_id = o.id
         LEFT JOIN org_quota q ON q.org_id = o.id
         LEFT JOIN plans p ON p.id = COALESCE(b.plan_id, o.plan_id)
         LEFT JOIN plans op ON op.id = o.plan_id
         WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;

    let limits = load_quota_limits(db, org_id).await?;
    let (projects_used, apps_used, services_used, members_used): (i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT
                (SELECT COUNT(*) FROM projects WHERE org_id = $1),
                (SELECT COUNT(*) FROM integration_apps WHERE org_id = $1),
                (SELECT COUNT(*) FROM services s JOIN projects p ON p.id = s.project_id WHERE p.org_id = $1),
                (SELECT COUNT(*) FROM org_members WHERE org_id = $1)",
        )
        .bind(org_id)
        .fetch_one(db)
        .await
        .map_err(db_err)?;

    Ok(OrgQuotaOut {
        org_id,
        external_id: external_id.unwrap_or_default(),
        plan,
        plan_id,
        sub_status: sub_status.unwrap_or_else(|| "active".to_string()),
        max_projects: limits.max_projects,
        max_members: limits.max_members,
        max_replicas: limits.max_replicas,
        max_parallel_deployments: limits.max_parallel_deployments,
        max_git_providers: limits.max_git_providers,
        max_orgs: limits.max_orgs,
        node_count: limits.node_count,
        cpu_cores: limits.cpu_cores,
        memory_mb: limits.memory_mb,
        projects_used,
        apps_used,
        services_used,
        members_used,
    })
}

async fn load(db: &sqlx::PgPool, org_id: Uuid, created: bool) -> Result<OrgOut, ApiAppError> {
    #[allow(clippy::type_complexity)]
    let (id, external_id, name, slug, plan, sub_status, created_at, integration, labels): (
        Uuid,
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        DateTime<Utc>,
        Option<String>,
        sqlx::types::Json<Labels>,
    ) = sqlx::query_as(
        "SELECT o.id, o.external_id, o.name, o.slug, COALESCE(p.name, op.name), b.sub_status, o.created_at, i.name, o.labels
         FROM organizations o
         LEFT JOIN org_billing b ON b.org_id = o.id
         LEFT JOIN plans p ON p.id = b.plan_id
         LEFT JOIN plans op ON op.id = o.plan_id
         LEFT JOIN integrations i ON i.id = o.integration_id
         WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;
    let quota = load_quota_limits(db, org_id).await?;
    Ok(OrgOut {
        id,
        external_id: external_id.unwrap_or_default(),
        name,
        slug,
        plan,
        sub_status: sub_status.unwrap_or_else(|| "active".to_string()),
        quota,
        created_at,
        integration,
        labels: labels.0,
        created,
    })
}

async fn apply_quota_patch_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: Uuid,
    patch: &QuotaPatch,
) -> Result<(), sqlx::Error> {
    let defaults = QuotaLimits::default();
    sqlx::query(
        r#"INSERT INTO org_quota (
               org_id, plan_id,
               max_projects, max_members, max_replicas, max_parallel_deployments,
               max_git_providers, max_orgs, node_count, cpu_cores, memory_mb,
               applied_at, updated_at
           )
           VALUES (
               $1,
               (SELECT plan_id FROM organizations WHERE id = $1),
               COALESCE($2, $11),
               COALESCE($3, $12),
               COALESCE($4, $13),
               COALESCE($5, $14),
               COALESCE($6, $15),
               COALESCE($7, $16),
               COALESCE($8, $17),
               COALESCE($9, $18),
               COALESCE($10, $19),
               NOW(), NOW()
           )
           ON CONFLICT (org_id) DO UPDATE SET
               max_projects             = COALESCE($2, org_quota.max_projects),
               max_members              = COALESCE($3, org_quota.max_members),
               max_replicas             = COALESCE($4, org_quota.max_replicas),
               max_parallel_deployments = COALESCE($5, org_quota.max_parallel_deployments),
               max_git_providers        = COALESCE($6, org_quota.max_git_providers),
               max_orgs                 = COALESCE($7, org_quota.max_orgs),
               node_count               = COALESCE($8, org_quota.node_count),
               cpu_cores                = COALESCE($9, org_quota.cpu_cores),
               memory_mb                = COALESCE($10, org_quota.memory_mb),
               updated_at               = NOW()"#,
    )
    .bind(org_id)
    .bind(patch.max_projects)
    .bind(patch.max_members)
    .bind(patch.max_replicas)
    .bind(patch.max_parallel_deployments)
    .bind(patch.max_git_providers)
    .bind(patch.max_orgs)
    .bind(patch.node_count)
    .bind(patch.cpu_cores)
    .bind(patch.memory_mb)
    .bind(defaults.max_projects)
    .bind(defaults.max_members)
    .bind(defaults.max_replicas)
    .bind(defaults.max_parallel_deployments)
    .bind(defaults.max_git_providers)
    .bind(defaults.max_orgs)
    .bind(defaults.node_count)
    .bind(defaults.cpu_cores)
    .bind(defaults.memory_mb)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_sub_status_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: Uuid,
    sub_status: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO org_billing (org_id, plan_id, tier, sub_status, updated_at)
           VALUES ($1, (SELECT plan_id FROM organizations WHERE id = $1), 'free'::subscription_tier, $2, NOW())
           ON CONFLICT (org_id) DO UPDATE
             SET sub_status = EXCLUDED.sub_status, updated_at = NOW()"#,
    )
    .bind(org_id)
    .bind(sub_status)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn apply_plan_and_quota_updates(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    org_id: Uuid,
    external_id: &str,
    plan: Option<&str>,
    sub_status: Option<&str>,
    quota_patch: &QuotaPatch,
) -> Result<(), ApiAppError> {
    if plan.is_none() && sub_status.is_none() && quota_patch.is_empty() {
        return Ok(());
    }
    // Plans and quotas decide limits and billing: only the integration that owns
    // the org (or a superadmin) may change them, never the org itself.
    if !caller.is_integration() && !is_superadmin(db, caller).await? {
        return Err(ApiAppError(AppError::Forbidden(
            "Only the integration that owns this organization can change its plan or quota".into(),
        )));
    }
    let normalized_status = match sub_status {
        Some(s) => Some(validate_sub_status(s)?),
        None => None,
    };
    quota_patch.validate()?;
    let resolved_plan = match plan {
        Some(p) => Some(plan_by_name(db, p).await?),
        None => None,
    };

    let mut tx = db.begin().await.map_err(db_err)?;
    if let Some((plan_id, ref plan_name)) = resolved_plan {
        sqlx::query("UPDATE organizations SET plan_id = $2 WHERE id = $1")
            .bind(org_id)
            .bind(plan_id)
            .execute(&mut *tx)
            .await
            .map_err(db_err)?;
        crate::orgs::apply_plan(&mut tx, org_id, plan_id, plan_name)
            .await
            .map_err(db_err)?;
    }
    if let Some(status) = normalized_status {
        apply_sub_status_in_tx(&mut tx, org_id, status)
            .await
            .map_err(db_err)?;
    }
    if !quota_patch.is_empty() {
        apply_quota_patch_in_tx(&mut tx, org_id, quota_patch)
            .await
            .map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;

    let updated = load_quota_out(db, org_id).await?;
    if resolved_plan.is_some() || normalized_status.is_some() {
        events::emit(
            db,
            Scope::org(org_id),
            "org.plan_changed",
            serde_json::json!({
                "plan": updated.plan,
                "sub_status": updated.sub_status,
            }),
        )
        .await;
    }
    if !quota_patch.is_empty() {
        events::emit(
            db,
            Scope::org(org_id),
            "org.quota_updated",
            serde_json::json!({
                "plan": updated.plan,
                "quota": {
                    "max_projects": updated.max_projects,
                    "max_members": updated.max_members,
                    "max_replicas": updated.max_replicas,
                    "max_parallel_deployments": updated.max_parallel_deployments,
                    "max_git_providers": updated.max_git_providers,
                    "max_orgs": updated.max_orgs,
                    "node_count": updated.node_count,
                    "cpu_cores": updated.cpu_cores,
                    "memory_mb": updated.memory_mb,
                },
            }),
        )
        .await;
    }
    caller
        .audit(
            db,
            "update_org_plan_or_quota",
            "organization",
            Some(org_id),
            serde_json::json!({
                "external_id": external_id,
                "plan": resolved_plan.as_ref().map(|(_, n)| n),
                "sub_status": normalized_status,
                "quota_patch": quota_patch,
            }),
        )
        .await;
    Ok(())
}

/// GET /plans — list all enabled Shipyard plans and their default resource quotas.
async fn list_plans(
    caller: IntegrationCaller,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<IntegrationPlanOut>>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let plans: Vec<IntegrationPlanOut> = sqlx::query_as(
        "SELECT id, name, cpu_cores, memory_mb, max_replicas, node_count,
                max_members, max_projects, max_orgs, max_parallel_deployments,
                max_git_providers, price_monthly
         FROM plans
         WHERE enabled = true
         ORDER BY price_monthly ASC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err)?;
    Ok(Json(ApiResponse::ok(plans)))
}

/// GET /orgs/:external_id/quota (and /orgs/:external_id/plan) — current plan, quota limits, and usage.
async fn get_org_quota(
    caller: IntegrationCaller,
    Path(external_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<OrgQuotaOut>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_external_id(&external_id)?;
    let org_id = org_by_external(&state.db, &caller, &external_id).await?;
    Ok(Json(ApiResponse::ok(
        load_quota_out(&state.db, org_id).await?,
    )))
}

/// PUT / PATCH /orgs/:external_id/quota (and /orgs/:external_id/plan) — upgrade/downgrade plan,
/// update billing sub_status, and/or override individual quota limits for an organization.
async fn update_org_quota(
    caller: IntegrationCaller,
    Path(external_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateOrgPlanOrQuota>,
) -> Result<Json<ApiResponse<OrgQuotaOut>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_external_id(&external_id)?;
    let org_id = resolve_or_create_org(&state.db, &caller, &external_id).await?;
    let merged_quota = body.flat_quota.merged_with(body.quota.as_ref());
    apply_plan_and_quota_updates(
        &state.db,
        &caller,
        org_id,
        &external_id,
        body.plan.as_deref(),
        body.sub_status.as_deref(),
        &merged_quota,
    )
    .await?;
    Ok(Json(ApiResponse::ok(
        load_quota_out(&state.db, org_id).await?,
    )))
}

/// Resolve an organization by its integration `external_id`, UUID, or slug; if not found and the
/// caller is an integration key (`ship_int_...`), auto-provision the organization on the default
/// `'free'` plan so one-shot endpoints (like `POST /orgs/:org/static-publish`) succeed even when
/// `PUT /orgs/:org` was not called first.
pub async fn resolve_or_create_org(
    db: &sqlx::PgPool,
    caller: &IntegrationCaller,
    external_id: &str,
) -> Result<Uuid, ApiAppError> {
    if let Ok(org_id) = org_by_external(db, caller, external_id).await {
        return Ok(org_id);
    }

    let integration_id = caller.integration_id().map_err(|_| {
        ApiAppError(AppError::NotFound(format!(
            "Organization '{external_id}' not found"
        )))
    })?;
    validate_external_id(external_id)?;

    // Never collide with or claim an existing unowned org that happens to share this UUID or slug.
    let external_uuid = Uuid::parse_str(external_id).ok();
    let conflict: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM organizations WHERE id = $1 OR slug = $2)",
    )
    .bind(external_uuid)
    .bind(external_id)
    .fetch_one(db)
    .await
    .map_err(db_err)?;
    if conflict {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Organization '{external_id}' not found"
        ))));
    }

    let (plan_id, plan_name) = plan_by_name(db, "free").await?;
    let org_id = Uuid::now_v7();
    let mut tx = db.begin().await.map_err(db_err)?;
    let inserted = sqlx::query(
        "INSERT INTO organizations (id, name, slug, plan_id, integration_id, external_id, labels, created_at)
         VALUES ($1, $2, $3, $4, $5, $6,
                 COALESCE((SELECT jsonb_build_object($7::text, name) FROM integrations WHERE id = $5), '{}'::jsonb), NOW())
         ON CONFLICT (integration_id, external_id) WHERE integration_id IS NOT NULL DO NOTHING",
    )
    .bind(org_id)
    .bind(external_id)
    .bind(org_slug(external_id))
    .bind(plan_id)
    .bind(integration_id)
    .bind(external_id)
    .bind(labels::INTEGRATION)
    .execute(&mut *tx)
    .await
    .map_err(db_err)?
    .rows_affected();

    if inserted == 0 {
        tx.rollback().await.ok();
        return org_by_external(db, caller, external_id).await;
    }

    crate::orgs::apply_plan(&mut tx, org_id, plan_id, &plan_name)
        .await
        .map_err(db_err)?;
    tx.commit().await.map_err(db_err)?;

    events::emit(
        db,
        Scope::org(org_id),
        "org.created",
        serde_json::json!({ "name": external_id, "plan": plan_name }),
    )
    .await;
    caller
        .audit(
            db,
            "create_org",
            "organization",
            Some(org_id),
            serde_json::json!({ "external_id": external_id, "plan": plan_name, "auto_created": true }),
        )
        .await;

    Ok(org_id)
}

/// PUT / PATCH /orgs/:external_id — create the org on first call, update its name,
/// plan, sub_status, quota, and/or labels afterwards. Safe to retry.
async fn upsert_org(
    caller: IntegrationCaller,
    Path(external_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpsertOrg>,
) -> Result<(StatusCode, Json<ApiResponse<OrgOut>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    validate_external_id(&external_id)?;
    let trimmed_name = body.name.as_deref().map(str::trim);
    if let Some(n) = trimmed_name {
        if n.is_empty() || n.len() > 100 {
            return Err(ApiAppError(AppError::BadRequest(
                "name must be 1-100 characters".into(),
            )));
        }
    }
    labels::validate(&body.labels).map_err(|e| ApiAppError(AppError::BadRequest(e)))?;
    let quota_patch = body.quota.clone().unwrap_or_default();

    if let Ok(org_id) = org_by_external(&state.db, &caller, &external_id).await {
        if let Some(name) = trimmed_name {
            sqlx::query("UPDATE organizations SET name = $2 WHERE id = $1")
                .bind(org_id)
                .bind(name)
                .execute(&state.db)
                .await
                .map_err(db_err)?;
        }
        labels::apply(&state.db, Table::Organizations, org_id, &body.labels)
            .await
            .map_err(db_err)?;
        apply_plan_and_quota_updates(
            &state.db,
            &caller,
            org_id,
            &external_id,
            body.plan.as_deref(),
            body.sub_status.as_deref(),
            &quota_patch,
        )
        .await?;
        return Ok((
            StatusCode::OK,
            Json(ApiResponse::ok(load(&state.db, org_id, false).await?)),
        ));
    }

    // Not found among the caller's orgs: creating one is a platform-level act.
    let integration_id = caller.integration_id().map_err(|_| {
        ApiAppError(AppError::Forbidden(
            "Only an integration key ('ship_int_...') can create organizations".into(),
        ))
    })?;
    let name = trimmed_name.ok_or_else(|| {
        ApiAppError(AppError::BadRequest(
            "name must be 1-100 characters".into(),
        ))
    })?;
    let normalized_status = match body.sub_status.as_deref() {
        Some(s) => Some(validate_sub_status(s)?),
        None => None,
    };
    quota_patch.validate()?;
    let (plan_id, plan_name) =
        plan_by_name(&state.db, body.plan.as_deref().unwrap_or("free")).await?;
    let org_id = Uuid::now_v7();
    let mut tx = state.db.begin().await.map_err(db_err)?;
    let inserted = sqlx::query(
        "INSERT INTO organizations (id, name, slug, plan_id, integration_id, external_id, labels, created_at)
         VALUES ($1, $2, $3, $4, $5, $6,
                 COALESCE((SELECT jsonb_build_object($7::text, name) FROM integrations WHERE id = $5), '{}'::jsonb), NOW())
         ON CONFLICT (integration_id, external_id) WHERE integration_id IS NOT NULL DO NOTHING",
    )
    .bind(org_id)
    .bind(name)
    .bind(org_slug(name))
    .bind(plan_id)
    .bind(integration_id)
    .bind(&external_id)
    .bind(labels::INTEGRATION)
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
    if let Some(status) = normalized_status {
        apply_sub_status_in_tx(&mut tx, org_id, status)
            .await
            .map_err(db_err)?;
    }
    if !quota_patch.is_empty() {
        apply_quota_patch_in_tx(&mut tx, org_id, &quota_patch)
            .await
            .map_err(db_err)?;
    }
    tx.commit().await.map_err(db_err)?;
    labels::apply(&state.db, Table::Organizations, org_id, &body.labels)
        .await
        .map_err(db_err)?;

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
            serde_json::json!({ "external_id": external_id, "plan": plan_name }),
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

/// Orgs with a teardown running in this process. A repeated DELETE while one
/// runs answers 202 again instead of starting a second, racing teardown.
static DELETING: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<Uuid>>> =
    std::sync::LazyLock::new(Default::default);

/// DELETE /orgs/:external_id — terminate every app (workloads, data, dev
/// databases, domains, secrets), then delete the org.
///
/// Answers 202 at once and tears down in the background: stopping every
/// stack of a large org takes longer than the edge proxies wait (Traefik
/// cuts requests at 60s, Cloudflare at 100s), which surfaced as a 502.
/// `org.deleted` is emitted when it finishes. If the teardown fails, the org
/// stays and a repeated DELETE picks up where it stopped.
async fn delete_org(
    caller: IntegrationCaller,
    Path(external_id): Path<String>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let org_id = org_by_external(&state.db, &caller, &external_id).await?;
    if !can_delete_org(&state.db, &caller, org_id).await? {
        return Err(ApiAppError(AppError::Forbidden(
            "Deleting an organization needs its integration key or an organization owner".into(),
        )));
    }

    let app_keys: Vec<String> =
        sqlx::query_scalar("SELECT app_key FROM integration_apps WHERE org_id = $1")
            .bind(org_id)
            .fetch_all(&state.db)
            .await
            .map_err(db_err)?;
    let accepted = || {
        (
            StatusCode::ACCEPTED,
            Json(ApiResponse::ok(serde_json::json!({
                "deleted": external_id,
                "status": "deleting",
                "apps_terminated": app_keys,
            }))),
        )
    };

    if !DELETING.lock().unwrap().insert(org_id) {
        return Ok(accepted());
    }
    caller
        .audit(
            &state.db,
            "delete_org",
            "organization",
            Some(org_id),
            serde_json::json!({ "external_id": external_id, "apps": app_keys }),
        )
        .await;

    let (task_state, task_caller, task_external) =
        (state.clone(), caller.clone(), external_id.clone());
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let result = run_org_deletion(&task_state, &task_caller, org_id, &task_external).await;
        DELETING.lock().unwrap().remove(&org_id);
        match result {
            Ok(()) => tracing::info!(
                %org_id, external_id = %task_external,
                "org deleted in {:.1}s", started.elapsed().as_secs_f64()
            ),
            Err(e) => tracing::error!(
                %org_id, external_id = %task_external,
                "org deletion failed after {:.1}s: {}", started.elapsed().as_secs_f64(), e.0
            ),
        }
    });

    Ok(accepted())
}

async fn run_org_deletion(
    state: &AppState,
    caller: &IntegrationCaller,
    org_id: Uuid,
    external_id: &str,
) -> Result<(), ApiAppError> {
    let app_keys: Vec<String> =
        sqlx::query_scalar("SELECT app_key FROM integration_apps WHERE org_id = $1")
            .bind(org_id)
            .fetch_all(&state.db)
            .await
            .map_err(db_err)?;
    for key in &app_keys {
        // An app terminated meanwhile by its own DELETE is already gone.
        let app = match super::app_ref(&state.db, caller, external_id, key).await {
            Ok(app) => app,
            Err(ApiAppError(AppError::NotFound(_))) => continue,
            Err(e) => return Err(e),
        };
        super::apps::terminate_app(state, &app).await?;
    }
    // Projects created outside the integration (e.g. by a member in the dashboard).
    let others: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM projects WHERE org_id = $1")
        .bind(org_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err)?;
    for project_id in &others {
        crate::projects::teardown::teardown_project(
            state,
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

    // Delete the organization's on-disk directory: <data_dir>/<org_id>
    let org_dir = format!("{}/{}", state.config.data_dir, org_id);
    let _ = tokio::fs::remove_dir_all(&org_dir).await;

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
    Ok(())
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

    #[test]
    fn quota_patch_validation_and_merging() {
        let empty = QuotaPatch::default();
        assert!(empty.is_empty());
        assert!(empty.validate().is_ok());

        let valid = QuotaPatch {
            max_projects: Some(20),
            max_replicas: Some(-1),
            cpu_cores: Some(4),
            memory_mb: Some(8192),
            ..Default::default()
        };
        assert!(!valid.is_empty());
        assert!(valid.validate().is_ok());

        let zero = QuotaPatch {
            max_projects: Some(0),
            ..Default::default()
        };
        assert!(zero.validate().is_err());

        let neg_two = QuotaPatch {
            memory_mb: Some(-2),
            ..Default::default()
        };
        assert!(neg_two.validate().is_err());

        // Nested quota overrides top-level fields when both are sent.
        let nested = QuotaPatch {
            max_projects: Some(50),
            max_members: Some(15),
            ..Default::default()
        };
        let merged = valid.merged_with(Some(&nested));
        assert_eq!(merged.max_projects, Some(50));
        assert_eq!(merged.max_members, Some(15));
        assert_eq!(merged.max_replicas, Some(-1));
        assert_eq!(merged.cpu_cores, Some(4));
        assert_eq!(merged.memory_mb, Some(8192));
    }

    #[test]
    fn sub_status_validation() {
        assert_eq!(validate_sub_status("active").ok(), Some("active"));
        assert_eq!(validate_sub_status("PAST_DUE").ok(), Some("past_due"));
        assert_eq!(validate_sub_status("canceled").ok(), Some("canceled"));
        assert_eq!(validate_sub_status("cancelled").ok(), Some("canceled"));
        assert_eq!(validate_sub_status("trialing").ok(), Some("trialing"));
        assert!(validate_sub_status("invalid").is_err());
    }

    #[test]
    fn update_payload_deserializes_flat_and_nested() {
        let flat: UpdateOrgPlanOrQuota = serde_json::from_value(serde_json::json!({
            "plan": "pro",
            "max_projects": 25,
            "cpu_cores": 4,
            "memory_mb": 8192
        }))
        .unwrap();
        let m1 = flat.flat_quota.merged_with(flat.quota.as_ref());
        assert_eq!(flat.plan.as_deref(), Some("pro"));
        assert_eq!(m1.max_projects, Some(25));
        assert_eq!(m1.cpu_cores, Some(4));
        assert_eq!(m1.memory_mb, Some(8192));

        let nested: UpdateOrgPlanOrQuota = serde_json::from_value(serde_json::json!({
            "plan": "max",
            "sub_status": "active",
            "quota": {
                "max_projects": -1,
                "max_replicas": 10
            }
        }))
        .unwrap();
        let m2 = nested.flat_quota.merged_with(nested.quota.as_ref());
        assert_eq!(nested.plan.as_deref(), Some("max"));
        assert_eq!(nested.sub_status.as_deref(), Some("active"));
        assert_eq!(m2.max_projects, Some(-1));
        assert_eq!(m2.max_replicas, Some(10));
    }
}
