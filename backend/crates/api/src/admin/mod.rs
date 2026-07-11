use axum::{
    body::Body,
    extract::{Path, Query, State},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use shipyard_common::{error::AppError, types::ApiResponse};
use crate::{auth::AuthUser, error::ApiAppError, AppState};

// ─── Admin access guards ──────────────────────────────────────────────────────

pub async fn require_superadmin(
    db: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<(), ApiAppError> {
    let is_super: Option<bool> = sqlx::query_scalar(
        "SELECT is_superadmin FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    match is_super {
        Some(true) => Ok(()),
        _ => Err(ApiAppError(AppError::Forbidden(
            "Super admin access required".to_string(),
        ))),
    }
}

/// Check that the user is either a superadmin or has the specific staff permission.
/// Superadmin bypasses all permission checks.
pub async fn require_admin_access(
    db: &sqlx::PgPool,
    user_id: Uuid,
    permission: &str,
) -> Result<(), ApiAppError> {
    let row: Option<(bool, Vec<String>)> = sqlx::query_as(
        "SELECT is_superadmin, staff_permissions FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    match row {
        Some((true, _)) => Ok(()),
        Some((false, perms)) if perms.contains(&permission.to_string()) => Ok(()),
        _ => Err(ApiAppError(AppError::Forbidden(
            "Admin access required".to_string(),
        ))),
    }
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/stats", get(get_stats))
        .route("/orgs", get(list_orgs))
        .route("/orgs/:org_id", patch(patch_org))
        .route("/orgs/:org_id/quota", get(get_org_quota).put(put_org_quota))
        .route("/users", get(list_users))
        .route("/users/:user_id", patch(patch_user))
        .route("/nodes", get(list_nodes))
        .route("/config", get(get_system_config))
        .route("/config/:key", patch(patch_system_config))
        .route("/audit-logs", get(list_all_audit_logs))
        // new routes
        .route("/projects", get(list_projects))
        .route("/deployments/app", get(list_app_deployments))
        .route("/deployments/provisioning", get(list_provisioning))
        .route("/infra/core-services", get(list_core_services))
        .route("/traefik/logs/stream", get(traefik_log_stream))
        .route("/plans", get(list_plans).post(create_plan))
        .route("/plans/:plan_id", patch(update_plan))
        .route("/staff", get(list_staff))
        .route("/staff/grant", post(grant_staff))
        .route("/staff/:user_id/revoke", post(revoke_staff))
        .route("/docker/prune/:what", post(docker_prune))
        .route("/mqtt/logs/stream", get(mqtt_log_stream))
        .route("/redis/info", get(get_redis_info))
        .route("/payments", get(list_payments))
}

// ─── GET /admin/stats ─────────────────────────────────────────────────────────

#[derive(Serialize)]
struct AdminStats {
    total_orgs: i64,
    total_users: i64,
    active_nodes: i64,
    paid_orgs: i64,
}

async fn get_stats(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<AdminStats>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?; // overview requires superadmin

    let total_orgs: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM organizations")
        .fetch_one(&state.db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    let total_users: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    let active_nodes: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM compute_nodes WHERE status = 'active'::node_status"
    ).fetch_one(&state.db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    let paid_orgs: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM org_billing WHERE tier != 'free' AND sub_status = 'active'"
    ).fetch_one(&state.db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(AdminStats {
        total_orgs: total_orgs.0,
        total_users: total_users.0,
        active_nodes: active_nodes.0,
        paid_orgs: paid_orgs.0,
    })))
}

// ─── GET /admin/orgs ──────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct AdminOrg {
    id: Uuid,
    name: String,
    slug: String,
    tier: Option<String>,
    sub_status: Option<String>,
    member_count: i64,
    node_count: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_orgs(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<AdminOrg>>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:organization:view").await?;

    let orgs: Vec<AdminOrg> = sqlx::query_as::<_, AdminOrg>(
        r#"SELECT
               o.id, o.name, o.slug, o.created_at,
               ob.tier::text AS tier,
               ob.sub_status,
               (SELECT COUNT(*) FROM org_members om WHERE om.org_id = o.id) AS member_count,
               (SELECT COUNT(*) FROM compute_nodes cn WHERE cn.org_id = o.id
                  AND cn.status NOT IN ('failed'::node_status, 'stopped'::node_status)) AS node_count
           FROM organizations o
           LEFT JOIN org_billing ob ON ob.org_id = o.id
           ORDER BY o.created_at DESC"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(orgs)))
}

// ─── PATCH /admin/orgs/:org_id ────────────────────────────────────────────────

#[derive(Deserialize)]
struct PatchOrgRequest {
    sub_status: Option<String>,
}

async fn patch_org(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Json(body): Json<PatchOrgRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:organization:manage").await?;

    if let Some(status) = &body.sub_status {
        sqlx::query(
            "UPDATE org_billing SET sub_status = $2, updated_at = NOW() WHERE org_id = $1"
        )
        .bind(org_id)
        .bind(status)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "updated": true }))))
}

// ─── GET /admin/orgs/:org_id/quota ────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AdminOrgQuota {
    org_id: Uuid,
    plan_id: Option<Uuid>,
    max_projects: i32,
    max_members: i32,
    max_replicas: i32,
    max_parallel_deployments: i32,
    max_git_providers: i32,
    max_orgs: i32,
    node_count: i32,
    cpu_cores: i32,
    memory_gb: i32,
}

async fn get_org_quota(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<AdminOrgQuota>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:organization:view").await?;

    let row: Option<AdminOrgQuota> = sqlx::query_as(
        "SELECT org_id, plan_id,
                max_projects, max_members, max_replicas, max_parallel_deployments,
                max_git_providers, max_orgs, node_count, cpu_cores, memory_gb
         FROM org_quota WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let quota = row.unwrap_or(AdminOrgQuota {
        org_id,
        plan_id: None,
        max_projects: 3,
        max_members: 5,
        max_replicas: 1,
        max_parallel_deployments: 1,
        max_git_providers: 1,
        max_orgs: 1,
        node_count: 1,
        cpu_cores: 1,
        memory_gb: 2,
    });

    Ok(Json(ApiResponse::ok(quota)))
}

// ─── PUT /admin/orgs/:org_id/quota ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PutOrgQuotaRequest {
    max_projects: i32,
    max_members: i32,
    max_replicas: i32,
    max_parallel_deployments: i32,
    max_git_providers: i32,
    max_orgs: i32,
    node_count: i32,
    cpu_cores: i32,
    memory_gb: i32,
}

async fn put_org_quota(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Json(body): Json<PutOrgQuotaRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:organization:manage").await?;

    sqlx::query(
        "INSERT INTO org_quota
             (org_id, max_projects, max_members, max_replicas, max_parallel_deployments,
              max_git_providers, max_orgs, node_count, cpu_cores, memory_gb,
              applied_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW(), NOW())
         ON CONFLICT (org_id) DO UPDATE SET
             max_projects             = EXCLUDED.max_projects,
             max_members              = EXCLUDED.max_members,
             max_replicas             = EXCLUDED.max_replicas,
             max_parallel_deployments = EXCLUDED.max_parallel_deployments,
             max_git_providers        = EXCLUDED.max_git_providers,
             max_orgs                 = EXCLUDED.max_orgs,
             node_count               = EXCLUDED.node_count,
             cpu_cores                = EXCLUDED.cpu_cores,
             memory_gb                = EXCLUDED.memory_gb,
             updated_at               = NOW()",
    )
    .bind(org_id)
    .bind(body.max_projects)
    .bind(body.max_members)
    .bind(body.max_replicas)
    .bind(body.max_parallel_deployments)
    .bind(body.max_git_providers)
    .bind(body.max_orgs)
    .bind(body.node_count)
    .bind(body.cpu_cores)
    .bind(body.memory_gb)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "updated": true }))))
}

// ─── GET /admin/users ─────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct AdminUser {
    id: Uuid,
    email: String,
    is_superadmin: bool,
    is_suspended: bool,
    org_count: i64,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_users(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<AdminUser>>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:users:view").await?;

    let users: Vec<AdminUser> = sqlx::query_as::<_, AdminUser>(
        r#"SELECT
               u.id, u.email, u.is_superadmin,
               COALESCE(u.is_suspended, FALSE) AS is_suspended,
               u.created_at,
               (SELECT COUNT(*) FROM org_members om WHERE om.user_id = u.id) AS org_count
           FROM users u
           ORDER BY u.created_at DESC"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(users)))
}

// ─── PATCH /admin/users/:user_id ──────────────────────────────────────────────

#[derive(Deserialize)]
struct PatchUserRequest {
    is_superadmin: Option<bool>,
    is_suspended: Option<bool>,
}

async fn patch_user(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<PatchUserRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:users:manage").await?;

    if user_id == auth.user_id {
        return Err(ApiAppError(AppError::Forbidden(
            "Cannot modify your own flags".to_string(),
        )));
    }

    if let Some(flag) = body.is_superadmin {
        sqlx::query("UPDATE users SET is_superadmin = $2, updated_at = NOW() WHERE id = $1")
            .bind(user_id)
            .bind(flag)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    if let Some(flag) = body.is_suspended {
        sqlx::query("UPDATE users SET is_suspended = $2, updated_at = NOW() WHERE id = $1")
            .bind(user_id)
            .bind(flag)
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "updated": true }))))
}

// ─── GET /admin/nodes ─────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct AdminNode {
    id: Uuid,
    org_id: Uuid,
    org_name: String,
    name: String,
    provider: String,
    region: String,
    status: String,
    public_ip: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_nodes(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<AdminNode>>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:nodes:view").await?;

    let nodes: Vec<AdminNode> = sqlx::query_as::<_, AdminNode>(
        r#"SELECT
               cn.id, cn.org_id, o.name AS org_name,
               cn.name, cn.provider, cn.region,
               cn.status::text AS status,
               cn.public_ip, cn.created_at
           FROM compute_nodes cn
           JOIN organizations o ON o.id = cn.org_id
           ORDER BY cn.created_at DESC"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(nodes)))
}

// ─── GET /admin/system ────────────────────────────────────────────────────────

async fn get_system_config(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT key, value FROM system_config ORDER BY key"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let map: serde_json::Map<String, serde_json::Value> = rows
        .into_iter()
        .map(|(k, v)| (k, v))
        .collect();

    Ok(Json(ApiResponse::ok(serde_json::Value::Object(map))))
}

// ─── PATCH /admin/system/:key ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct PatchSystemConfigRequest {
    value: serde_json::Value,
}

async fn patch_system_config(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(key): Path<String>,
    Json(body): Json<PatchSystemConfigRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    sqlx::query(
        "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, NOW())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()"
    )
    .bind(&key)
    .bind(&body.value)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "updated": true }))))
}

// ─── GET /admin/audit-logs ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct AdminAuditParams {
    cursor: Option<String>,
    limit:  Option<i64>,
    org_id: Option<Uuid>,
}

async fn list_all_audit_logs(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<AdminAuditParams>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:audit:view").await?;

    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let fetch = limit + 1;

    #[derive(sqlx::FromRow)]
    struct AuditRow {
        id:            Uuid,
        user_id:       Option<Uuid>,
        action:        String,
        resource_type: String,
        resource_id:   Option<Uuid>,
        ip_address:    Option<String>,
        metadata:      Option<serde_json::Value>,
        created_at:    chrono::DateTime<chrono::Utc>,
    }

    let rows: Vec<AuditRow> = match (&params.cursor, &params.org_id) {
        (Some(cursor), Some(org_id)) => {
            let ts = cursor.parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|_| ApiAppError(AppError::BadRequest("Invalid cursor".to_string())))?;
            sqlx::query_as(
                r#"SELECT al.id, al.user_id, al.action, al.resource_type, al.resource_id,
                          al.ip_address, al.metadata, al.created_at
                   FROM audit_logs al
                   WHERE ((al.resource_type = 'org' AND al.resource_id = $1)
                      OR al.user_id IN (SELECT user_id FROM org_members WHERE org_id = $1))
                     AND al.created_at < $2
                   ORDER BY al.created_at DESC LIMIT $3"#,
            )
            .bind(org_id).bind(ts).bind(fetch)
            .fetch_all(&state.db).await
        }
        (None, Some(org_id)) => {
            sqlx::query_as(
                r#"SELECT al.id, al.user_id, al.action, al.resource_type, al.resource_id,
                          al.ip_address, al.metadata, al.created_at
                   FROM audit_logs al
                   WHERE (al.resource_type = 'org' AND al.resource_id = $1)
                      OR al.user_id IN (SELECT user_id FROM org_members WHERE org_id = $1)
                   ORDER BY al.created_at DESC LIMIT $2"#,
            )
            .bind(org_id).bind(fetch)
            .fetch_all(&state.db).await
        }
        (Some(cursor), None) => {
            let ts = cursor.parse::<chrono::DateTime<chrono::Utc>>()
                .map_err(|_| ApiAppError(AppError::BadRequest("Invalid cursor".to_string())))?;
            sqlx::query_as(
                r#"SELECT id, user_id, action, resource_type, resource_id,
                          ip_address, metadata, created_at
                   FROM audit_logs
                   WHERE created_at < $1
                   ORDER BY created_at DESC LIMIT $2"#,
            )
            .bind(ts).bind(fetch)
            .fetch_all(&state.db).await
        }
        (None, None) => {
            sqlx::query_as(
                r#"SELECT id, user_id, action, resource_type, resource_id,
                          ip_address, metadata, created_at
                   FROM audit_logs
                   ORDER BY created_at DESC LIMIT $1"#,
            )
            .bind(fetch)
            .fetch_all(&state.db).await
        }
    }
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let has_next = rows.len() > limit as usize;
    let page = if has_next { &rows[..limit as usize] } else { &rows[..] };
    let next_cursor = if has_next {
        page.last().map(|r| r.created_at.to_rfc3339())
    } else {
        None
    };

    let items: Vec<serde_json::Value> = page.iter().map(|r| serde_json::json!({
        "id":            r.id,
        "user_id":       r.user_id,
        "action":        r.action,
        "resource_type": r.resource_type,
        "resource_id":   r.resource_id,
        "ip_address":    r.ip_address,
        "metadata":      r.metadata,
        "created_at":    r.created_at,
    })).collect();

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "items":       items,
        "next_cursor": next_cursor,
    }))))
}

// ─── GET /admin/projects ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PaginationSearch {
    page:  Option<i64>,
    limit: Option<i64>,
    q:     Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct AdminProject {
    id:            Uuid,
    name:          String,
    slug:          String,
    org_id:        Uuid,
    org_name:      String,
    org_slug:      String,
    service_count: i64,
    created_at:    chrono::DateTime<chrono::Utc>,
}

async fn list_projects(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<PaginationSearch>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:projects:view").await?;

    let limit  = params.limit.unwrap_or(25).clamp(1, 100);
    let offset = params.page.unwrap_or(0) * limit;
    let q      = params.q.as_deref().unwrap_or("").to_string();
    let search = format!("%{}%", q);

    let total: (i64,) = if q.is_empty() {
        sqlx::query_as("SELECT COUNT(*) FROM projects")
            .fetch_one(&state.db).await
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM projects WHERE name ILIKE $1 OR slug ILIKE $1")
            .bind(&search)
            .fetch_one(&state.db).await
    }
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let items: Vec<AdminProject> = if q.is_empty() {
        sqlx::query_as::<_, AdminProject>(
            r#"SELECT p.id, p.name, p.slug, p.org_id, p.created_at,
                      o.name AS org_name, o.slug AS org_slug,
                      (SELECT COUNT(*) FROM services s WHERE s.project_id = p.id) AS service_count
               FROM projects p
               JOIN organizations o ON o.id = p.org_id
               ORDER BY p.created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(limit).bind(offset)
        .fetch_all(&state.db).await
    } else {
        sqlx::query_as::<_, AdminProject>(
            r#"SELECT p.id, p.name, p.slug, p.org_id, p.created_at,
                      o.name AS org_name, o.slug AS org_slug,
                      (SELECT COUNT(*) FROM services s WHERE s.project_id = p.id) AS service_count
               FROM projects p
               JOIN organizations o ON o.id = p.org_id
               WHERE p.name ILIKE $1 OR p.slug ILIKE $1
               ORDER BY p.created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(&search).bind(limit).bind(offset)
        .fetch_all(&state.db).await
    }
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "items": items,
        "total": total.0,
        "page":  params.page.unwrap_or(0),
        "limit": limit,
    }))))
}

// ─── GET /admin/deployments/app ──────────────────────────────────────────────

#[derive(Deserialize)]
struct DeploymentParams {
    page:   Option<i64>,
    limit:  Option<i64>,
    org_id: Option<Uuid>,
    status: Option<String>,
}

async fn list_app_deployments(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<DeploymentParams>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:deployments:projects:view").await?;

    let limit  = params.limit.unwrap_or(30).clamp(1, 100);
    let offset = params.page.unwrap_or(0) * limit;

    #[derive(sqlx::FromRow, Serialize)]
    struct DeployRow {
        id:           Uuid,
        service_id:   Uuid,
        service_name: String,
        org_id:       Uuid,
        org_name:     String,
        triggered_by: String,
        source_ref:   String,
        status:       String,
        created_at:   chrono::DateTime<chrono::Utc>,
        finished_at:  Option<chrono::DateTime<chrono::Utc>>,
    }

    let items: Vec<DeployRow> = sqlx::query_as::<_, DeployRow>(
        r#"SELECT
               d.id, d.triggered_by, d.source_ref,
               d.status::text AS status,
               d.created_at, d.finished_at,
               sv.id AS service_id, sv.name AS service_name,
               o.id AS org_id, o.name AS org_name
           FROM deployments d
           JOIN services sv ON sv.id = d.service_id
           JOIN projects p  ON p.id  = sv.project_id
           JOIN organizations o ON o.id = p.org_id
           WHERE ($1::uuid IS NULL OR o.id = $1)
             AND ($2::text IS NULL OR d.status::text = $2)
           ORDER BY d.created_at DESC
           LIMIT $3 OFFSET $4"#,
    )
    .bind(params.org_id)
    .bind(params.status.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "items": items,
        "page":  params.page.unwrap_or(0),
        "limit": limit,
    }))))
}

// ─── GET /admin/deployments/provisioning ─────────────────────────────────────

#[derive(Deserialize)]
struct ProvisioningParams {
    page:  Option<i64>,
    limit: Option<i64>,
}

async fn list_provisioning(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<ProvisioningParams>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:deployments:orgs:view").await?;

    let limit  = params.limit.unwrap_or(30).clamp(1, 100);
    let offset = params.page.unwrap_or(0) * limit;

    #[derive(sqlx::FromRow, Serialize)]
    struct ProvRow {
        id:         Uuid,
        name:       String,
        provider:   String,
        region:     String,
        status:     String,
        org_id:     Uuid,
        org_name:   String,
        created_at: chrono::DateTime<chrono::Utc>,
    }

    let items: Vec<ProvRow> = sqlx::query_as::<_, ProvRow>(
        r#"SELECT
               cn.id, cn.name, cn.provider, cn.region,
               cn.status::text AS status,
               o.id AS org_id, o.name AS org_name,
               cn.created_at
           FROM compute_nodes cn
           JOIN organizations o ON o.id = cn.org_id
           WHERE cn.status NOT IN ('active'::node_status, 'stopped'::node_status)
           ORDER BY cn.created_at DESC
           LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "items": items,
        "page":  params.page.unwrap_or(0),
        "limit": limit,
    }))))
}

// ─── GET /admin/infra/core-services ──────────────────────────────────────────

async fn list_core_services(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<serde_json::Value>>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:view").await?;

    let containers = state.docker.list_all_containers().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;

    let services: Vec<serde_json::Value> = containers.into_iter()
        .filter(|c| {
            let name = c.names.first().cloned().unwrap_or_default().to_lowercase();
            let trimmed = name.trim_start_matches('/');
            trimmed.starts_with("shipyard-") || trimmed.starts_with("shipyard_")
        })
        .map(|c| {
            let name = c.names.first().cloned().unwrap_or_default();
            serde_json::json!({
                "id":     c.id,
                "name":   name,
                "image":  c.image,
                "status": c.status,
                "state":  c.state,
            })
        })
        .collect();

    Ok(Json(ApiResponse::ok(services)))
}

// ─── GET /admin/traefik/logs/stream (SSE) ────────────────────────────────────

async fn traefik_log_stream(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Response<Body>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    use futures::StreamExt;
    use axum::http::header;

    let log_path = "/var/log/traefik/access.log";
    let content  = tokio::fs::read_to_string(log_path).await.unwrap_or_default();

    let lines: Vec<String> = content
        .lines()
        .rev()
        .take(200)
        .map(|l| format!("data: {}\n\n", l))
        .collect();

    let stream = futures::stream::iter(
        lines.into_iter().map(Ok::<_, std::convert::Infallible>)
    );

    let body = Body::from_stream(stream);

    Ok((
        [(header::CONTENT_TYPE, "text/event-stream"),
         (header::CACHE_CONTROL, "no-cache"),
         (header::CONNECTION, "keep-alive")],
        body,
    ).into_response())
}

// ─── GET /admin/plans ─────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct Plan {
    id:                       Uuid,
    name:                     String,
    enabled:                  bool,
    cpu_cores:                i32,
    memory_gb:                i32,
    max_replicas:             i32,
    node_count:               i32,
    max_members:              i32,
    max_projects:             i32,
    max_orgs:                 i32,
    max_parallel_deployments: i32,
    max_git_providers:        i32,
    price_monthly:            f64,
    created_at:               chrono::DateTime<chrono::Utc>,
    updated_at:               chrono::DateTime<chrono::Utc>,
}

async fn list_plans(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Plan>>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:plan:view").await?;

    let plans: Vec<Plan> = sqlx::query_as::<_, Plan>(
        "SELECT * FROM plans ORDER BY price_monthly ASC"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(plans)))
}

// ─── POST /admin/plans ────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreatePlanRequest {
    name:                     String,
    enabled:                  Option<bool>,
    cpu_cores:                Option<i32>,
    memory_gb:                Option<i32>,
    max_replicas:             Option<i32>,
    node_count:               Option<i32>,
    max_members:              Option<i32>,
    max_projects:             Option<i32>,
    max_orgs:                 Option<i32>,
    max_parallel_deployments: Option<i32>,
    max_git_providers:        Option<i32>,
    price_monthly:            Option<f64>,
}

async fn create_plan(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreatePlanRequest>,
) -> Result<Json<ApiResponse<Plan>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:plan:manage").await?;

    let plan: Plan = sqlx::query_as::<_, Plan>(
        r#"INSERT INTO plans
               (name, enabled, cpu_cores, memory_gb, max_replicas, node_count,
                max_members, max_projects, max_orgs, max_parallel_deployments,
                max_git_providers, price_monthly)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)
           RETURNING *"#,
    )
    .bind(&body.name)
    .bind(body.enabled.unwrap_or(true))
    .bind(body.cpu_cores.unwrap_or(2))
    .bind(body.memory_gb.unwrap_or(4))
    .bind(body.max_replicas.unwrap_or(3))
    .bind(body.node_count.unwrap_or(1))
    .bind(body.max_members.unwrap_or(5))
    .bind(body.max_projects.unwrap_or(10))
    .bind(body.max_orgs.unwrap_or(1))
    .bind(body.max_parallel_deployments.unwrap_or(1))
    .bind(body.max_git_providers.unwrap_or(1))
    .bind(body.price_monthly.unwrap_or(0.0))
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(plan)))
}

// ─── PATCH /admin/plans/:plan_id ─────────────────────────────────────────────

#[derive(Deserialize)]
struct UpdatePlanRequest {
    enabled:                  Option<bool>,
    cpu_cores:                Option<i32>,
    memory_gb:                Option<i32>,
    max_replicas:             Option<i32>,
    node_count:               Option<i32>,
    max_members:              Option<i32>,
    max_projects:             Option<i32>,
    max_orgs:                 Option<i32>,
    max_parallel_deployments: Option<i32>,
    max_git_providers:        Option<i32>,
    price_monthly:            Option<f64>,
}

async fn update_plan(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(plan_id): Path<Uuid>,
    Json(body): Json<UpdatePlanRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:plan:manage").await?;

    if let Some(v) = body.enabled {
        sqlx::query("UPDATE plans SET enabled = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.cpu_cores {
        sqlx::query("UPDATE plans SET cpu_cores = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.memory_gb {
        sqlx::query("UPDATE plans SET memory_gb = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.max_replicas {
        sqlx::query("UPDATE plans SET max_replicas = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.node_count {
        sqlx::query("UPDATE plans SET node_count = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.max_members {
        sqlx::query("UPDATE plans SET max_members = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.max_projects {
        sqlx::query("UPDATE plans SET max_projects = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.max_orgs {
        sqlx::query("UPDATE plans SET max_orgs = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.max_parallel_deployments {
        sqlx::query("UPDATE plans SET max_parallel_deployments = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.max_git_providers {
        sqlx::query("UPDATE plans SET max_git_providers = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = body.price_monthly {
        sqlx::query("UPDATE plans SET price_monthly = $2, updated_at = NOW() WHERE id = $1")
            .bind(plan_id).bind(v).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "updated": true }))))
}

// ─── POST /admin/staff/grant ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct GrantStaffRequest {
    email:       String,
    permissions: Option<Vec<String>>,
}

async fn grant_staff(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<GrantStaffRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:staff:manage").await?;

    let user_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE email = $1"
    )
    .bind(&body.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let uid = user_id.ok_or_else(|| ApiAppError(AppError::NotFound(
        format!("User with email {} not found", body.email)
    )))?;

    let perms = body.permissions.unwrap_or_default();
    if perms.is_empty() {
        return Err(ApiAppError(AppError::BadRequest(
            "At least one permission must be selected".to_string(),
        )));
    }

    sqlx::query(
        "UPDATE users SET staff_permissions = $2, updated_at = NOW() WHERE id = $1"
    )
    .bind(uid)
    .bind(&perms)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "granted": true,
        "user_id": uid,
        "permissions": perms,
    }))))
}

// ─── GET /admin/staff ────────────────────────────────────────────────────────

#[derive(Serialize, sqlx::FromRow)]
struct StaffUser {
    id:                Uuid,
    email:             String,
    staff_permissions: Vec<String>,
    created_at:        chrono::DateTime<chrono::Utc>,
}

async fn list_staff(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StaffUser>>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:staff:view").await?;

    let staff: Vec<StaffUser> = sqlx::query_as::<_, StaffUser>(
        r#"SELECT id, email, staff_permissions, created_at
           FROM users
           WHERE cardinality(staff_permissions) > 0
           ORDER BY created_at DESC"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(staff)))
}

// ─── POST /admin/staff/:user_id/revoke ───────────────────────────────────────

async fn revoke_staff(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:staff:manage").await?;

    sqlx::query("UPDATE users SET staff_permissions = '{}', updated_at = NOW() WHERE id = $1")
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "revoked": true }))))
}

// ─── GET /admin/redis/info ────────────────────────────────────────────────────

#[derive(Serialize)]
struct RedisInfoEntry {
    key:   String,
    value: String,
}

async fn get_redis_info(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<RedisInfoEntry>>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:redis:view").await?;

    let mut conn = match state.redis.clone() {
        Some(c) => c,
        None => return Err(ApiAppError(AppError::Internal("Redis not configured".to_string()))),
    };

    use redis::AsyncCommands;
    let raw: String = redis::cmd("INFO")
        .arg("all")
        .query_async(&mut conn)
        .await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;

    let entries: Vec<RedisInfoEntry> = raw
        .lines()
        .filter(|l| !l.starts_with('#') && l.contains(':'))
        .map(|l| {
            let mut parts = l.splitn(2, ':');
            let key   = parts.next().unwrap_or("").trim().to_string();
            let value = parts.next().unwrap_or("").trim().to_string();
            RedisInfoEntry { key, value }
        })
        .collect();

    Ok(Json(ApiResponse::ok(entries)))
}

// ─── GET /admin/mqtt/logs/stream (SSE) ───────────────────────────────────────

async fn mqtt_log_stream(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Response<Body>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    use axum::http::header;

    let log_paths = [
        "/var/log/mosquitto/mosquitto.log",
        "/mosquitto/log/mosquitto.log",
        "/var/log/mosquitto.log",
    ];

    let mut content = String::new();
    for path in &log_paths {
        if let Ok(c) = tokio::fs::read_to_string(path).await {
            content = c;
            break;
        }
    }

    let lines: Vec<String> = content
        .lines()
        .rev()
        .take(200)
        .map(|l| format!("data: {}\n\n", l))
        .collect();

    let stream = futures::stream::iter(
        lines.into_iter().map(Ok::<_, std::convert::Infallible>)
    );

    let body = Body::from_stream(stream);

    Ok((
        [(header::CONTENT_TYPE, "text/event-stream"),
         (header::CACHE_CONTROL, "no-cache"),
         (header::CONNECTION, "keep-alive")],
        body,
    ).into_response())
}

// ─── POST /admin/docker/prune/:what ──────────────────────────────────────────

async fn docker_prune(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(what): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_superadmin(&state.db, auth.user_id).await?;

    let freed = match what.as_str() {
        "containers" => state.docker.prune_containers().await
            .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?,
        "volumes" => state.docker.prune_volumes().await
            .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?,
        "images" => state.docker.prune_images().await
            .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?,
        _ => return Err(ApiAppError(AppError::BadRequest(
            format!("Unknown prune target: {}. Use containers, volumes, or images", what)
        ))),
    };

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "pruned": what,
        "space_reclaimed_bytes": freed,
    }))))
}

// ─── GET /admin/payments ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PaymentQuery {
    status: Option<String>,
    page: Option<i64>,
    per_page: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct PaymentRow {
    id:                       Uuid,
    org_id:                   Option<Uuid>,
    org_name:                 Option<String>,
    stripe_payment_intent_id: Option<String>,
    amount:                   i32,
    currency:                 String,
    status:                   String,
    description:              Option<String>,
    created_at:               chrono::DateTime<chrono::Utc>,
}

async fn list_payments(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<PaymentQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:billing:view").await?;

    let per_page = q.per_page.unwrap_or(25).clamp(1, 100);
    let page     = q.page.unwrap_or(0).max(0);
    let offset   = page * per_page;

    let status_filter = q.status.as_deref().unwrap_or("");

    let rows: Vec<PaymentRow> = if status_filter.is_empty() {
        sqlx::query_as(
            r#"SELECT p.id, p.org_id, o.name AS org_name,
                      p.stripe_payment_intent_id, p.amount, p.currency,
                      p.status, p.description, p.created_at
               FROM payments p
               LEFT JOIN organizations o ON o.id = p.org_id
               ORDER BY p.created_at DESC
               LIMIT $1 OFFSET $2"#,
        )
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    } else {
        sqlx::query_as(
            r#"SELECT p.id, p.org_id, o.name AS org_name,
                      p.stripe_payment_intent_id, p.amount, p.currency,
                      p.status, p.description, p.created_at
               FROM payments p
               LEFT JOIN organizations o ON o.id = p.org_id
               WHERE p.status = $1
               ORDER BY p.created_at DESC
               LIMIT $2 OFFSET $3"#,
        )
        .bind(status_filter)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    };

    let total: (i64,) = if status_filter.is_empty() {
        sqlx::query_as("SELECT COUNT(*) FROM payments")
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    } else {
        sqlx::query_as("SELECT COUNT(*) FROM payments WHERE status = $1")
            .bind(status_filter)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    };

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "items": rows,
        "total": total.0,
        "page": page,
        "per_page": per_page,
    }))))
}

