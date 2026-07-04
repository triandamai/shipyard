use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    routing::{delete, get, post, put},
    Json, Router,
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::{ApiResponse, PaginationParams};
use shipyard_db::models::{Invitation, OrgMember, Organization};
use shipyard_db::redis as session;

use crate::auth::{AuthUser, create_access_token, create_refresh_token};
use crate::error::ApiAppError;
use crate::AppState;

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOrgRequest {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ProjectAssignment {
    pub project_id: Uuid,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct InviteMemberRequest {
    pub email: String,
    pub role: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub project_assignments: Vec<ProjectAssignment>,
}

#[derive(Debug, Deserialize)]
pub struct ChangeRoleRequest {
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct SetPermissionsRequest {
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct OrgResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: chrono::DateTime<Utc>,
}

impl From<Organization> for OrgResponse {
    fn from(o: Organization) -> Self {
        Self {
            id: o.id,
            name: o.name,
            slug: o.slug,
            created_at: o.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub created_at: chrono::DateTime<Utc>,
}

impl From<OrgMember> for MemberResponse {
    fn from(m: OrgMember) -> Self {
        Self {
            id: m.id,
            org_id: m.org_id,
            user_id: m.user_id,
            email: String::new(),
            role: m.role,
            permissions: vec![],
            created_at: m.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct InvitationResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub email: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub project_assignments: serde_json::Value,
    pub token: String,
    pub expires_at: chrono::DateTime<Utc>,
    pub accepted_at: Option<chrono::DateTime<Utc>>,
    pub created_at: chrono::DateTime<Utc>,
}

impl From<Invitation> for InvitationResponse {
    fn from(i: Invitation) -> Self {
        Self {
            id: i.id,
            org_id: i.org_id,
            email: i.email,
            role: i.role,
            permissions: i.permissions,
            project_assignments: i.project_assignments,
            token: i.token,
            expires_at: i.expires_at,
            accepted_at: i.accepted_at,
            created_at: i.created_at,
        }
    }
}

/// Public invitation details returned without authentication.
#[derive(Debug, Serialize)]
pub struct PublicInviteResponse {
    pub token: String,
    pub org_id: Uuid,
    pub org_name: String,
    pub email: String,
    pub role: String,
    pub permissions: Vec<String>,
    pub project_assignments: serde_json::Value,
    pub expires_at: chrono::DateTime<Utc>,
    pub is_expired: bool,
    pub is_accepted: bool,
}

/// Body for completing an invitation — creates a new account and logs in.
#[derive(Debug, Deserialize)]
pub struct CompleteInviteRequest {
    pub password: String,
}

/// Response after completing an invitation (mirrors login response structure).
#[derive(Debug, Serialize)]
pub struct CompleteInviteResponse {
    pub access_token: String,
    pub user_id: Uuid,
    pub email: String,
}

// ─── Routers ─────────────────────────────────────────────────────────────────

/// Authenticated routes — mounted under /orgs with JWT middleware.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_orgs).post(create_org))
        .route("/:org_id", get(get_org).put(update_org).delete(delete_org))
        .route("/:org_id/members", get(list_members))
        .route("/:org_id/members/me", get(get_my_membership))
        .route("/:org_id/members/invite", post(invite_member))
        .route("/:org_id/members/:user_id/role", put(change_member_role))
        .route("/:org_id/members/:user_id/permissions", put(set_member_permissions))
        .route("/:org_id/members/:user_id/projects", get(get_member_projects).put(set_member_projects))
        .route("/:org_id/members/:user_id", delete(remove_member))
        .route("/:org_id/invitations", get(list_invitations))
        .route("/:org_id/invitations/:invitation_id", delete(cancel_invitation))
        .route("/:org_id/invitations/:token/accept", post(accept_invitation))
        .route("/:org_id/audit-logs", get(list_audit_logs))
}

/// Public routes — no JWT required.  Mounted under /invite at the root level.
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/:token", get(get_invite_public))
        .route("/:token/complete", post(complete_invite))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn validate_role(role: &str) -> Result<(), ApiAppError> {
    match role {
        "owner" | "admin" | "member" | "viewer" => Ok(()),
        _ => Err(ApiAppError(AppError::Validation(format!(
            "Invalid role '{}'. Must be one of: owner, admin, member, viewer",
            role
        )))),
    }
}

fn validate_permissions(perms: &[String]) -> Result<(), ApiAppError> {
    const VALID_PERMISSIONS: &[&str] = &[
        "app:org:settings:read",
        "app:org:settings:write",
        "app:org:members:read",
        "app:org:members:invite",
        "app:org:members:manage",
        "app:project:read",
        "app:project:write",
        "app:project:delete",
        "app:project:service:read",
        "app:project:service:write",
        "app:project:service:deploy",
        "app:project:service:delete",
        "app:project:volume:read",
        "app:project:volume:write",
        "app:project:network:read",
        "app:project:network:write",
        "app:project:domain:read",
        "app:project:domain:write",
    ];
    for p in perms {
        if !VALID_PERMISSIONS.contains(&p.as_str()) {
            return Err(ApiAppError(AppError::Validation(format!(
                "Unknown permission: '{}'",
                p
            ))));
        }
    }
    Ok(())
}

fn validate_project_permissions(perms: &[String]) -> Result<(), ApiAppError> {
    const VALID: &[&str] = &["view", "deploy", "manage"];
    for p in perms {
        if !VALID.contains(&p.as_str()) {
            return Err(ApiAppError(AppError::Validation(format!(
                "Unknown project permission '{}'. Valid values: view, deploy, manage",
                p
            ))));
        }
    }
    Ok(())
}

async fn require_member(
    db: &sqlx::PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<OrgMember, ApiAppError> {
    sqlx::query_as::<_, OrgMember>(
        "SELECT id, org_id, user_id, role::text AS role, created_at
         FROM org_members
         WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::Forbidden(
        "You are not a member of this organization".to_string(),
    )))
}

async fn require_admin(
    db: &sqlx::PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<OrgMember, ApiAppError> {
    let member = require_member(db, org_id, user_id).await?;
    match member.role.as_str() {
        "owner" | "admin" => Ok(member),
        _ => Err(ApiAppError(AppError::Forbidden(
            "Only owners and admins can perform this action".to_string(),
        ))),
    }
}

async fn require_owner(
    db: &sqlx::PgPool,
    org_id: Uuid,
    user_id: Uuid,
) -> Result<OrgMember, ApiAppError> {
    let member = require_member(db, org_id, user_id).await?;
    if member.role != "owner" {
        return Err(ApiAppError(AppError::Forbidden(
            "Only the owner can perform this action".to_string(),
        )));
    }
    Ok(member)
}

fn set_refresh_cookie(token: &str, max_age: u64) -> axum::http::HeaderValue {
    let value = format!(
        "shipyard_refresh={token}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={max_age}"
    );
    axum::http::HeaderValue::from_str(&value)
        .unwrap_or_else(|_| axum::http::HeaderValue::from_static(""))
}

/// Apply project_assignments from an invitation to the `project_members` table.
async fn apply_project_assignments(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    org_id: Uuid,
    user_id: Uuid,
    project_assignments: &serde_json::Value,
    invited_by: Option<Uuid>,
) -> Result<(), ApiAppError> {
    let assignments: Vec<ProjectAssignment> = match serde_json::from_value(project_assignments.clone()) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };

    for assignment in assignments {
        sqlx::query(
            r#"
            INSERT INTO project_members (id, project_id, org_id, user_id, permissions, invited_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            ON CONFLICT (project_id, user_id) DO UPDATE
                SET permissions = EXCLUDED.permissions
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(assignment.project_id)
        .bind(org_id)
        .bind(user_id)
        .bind(&assignment.permissions)
        .bind(invited_by)
        .execute(&mut **tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    Ok(())
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn list_orgs(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<OrgResponse>>>, ApiAppError> {
    let orgs: Vec<Organization> = sqlx::query_as::<_, Organization>(
        "SELECT o.id, o.name, o.slug, o.created_at
         FROM organizations o
         INNER JOIN org_members m ON m.org_id = o.id
         WHERE m.user_id = $1
         ORDER BY o.name ASC",
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(
        orgs.into_iter().map(OrgResponse::from).collect(),
    )))
}

async fn create_org(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateOrgRequest>,
) -> Result<(StatusCode, Json<ApiResponse<OrgResponse>>), ApiAppError> {
    if body.name.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("Organization name is required".to_string())));
    }
    if body.slug.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("Organization slug is required".to_string())));
    }

    let existing_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM organizations WHERE slug = $1")
            .bind(&body.slug)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if existing_count.0 > 0 {
        return Err(ApiAppError(AppError::Conflict(format!("Slug '{}' is already taken", body.slug))));
    }

    let org_id = Uuid::now_v7();
    let mut tx = state.db.begin().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let org: Organization = sqlx::query_as::<_, Organization>(
        "INSERT INTO organizations (id, name, slug, created_at)
         VALUES ($1, $2, $3, NOW())
         RETURNING id, name, slug, created_at",
    )
    .bind(org_id)
    .bind(&body.name)
    .bind(&body.slug)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    sqlx::query(
        "INSERT INTO org_members (id, org_id, user_id, role, created_at)
         VALUES ($1, $2, $3, 'owner'::member_role, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    tx.commit().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(OrgResponse::from(org)))))
}

async fn get_org(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<OrgResponse>>, ApiAppError> {
    require_member(&state.db, org_id, auth.user_id).await?;

    let org: Organization = sqlx::query_as::<_, Organization>(
        "SELECT id, name, slug, created_at FROM organizations WHERE id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Organization not found".to_string())))?;

    Ok(Json(ApiResponse::ok(OrgResponse::from(org))))
}

async fn update_org(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Json(body): Json<UpdateOrgRequest>,
) -> Result<Json<ApiResponse<OrgResponse>>, ApiAppError> {
    if body.name.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("Organization name is required".to_string())));
    }
    require_admin(&state.db, org_id, auth.user_id).await?;

    let org: Organization = sqlx::query_as::<_, Organization>(
        "UPDATE organizations SET name = $2 WHERE id = $1
         RETURNING id, name, slug, created_at",
    )
    .bind(org_id)
    .bind(&body.name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Organization not found".to_string())))?;

    Ok(Json(ApiResponse::ok(OrgResponse::from(org))))
}

async fn delete_org(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_owner(&state.db, org_id, auth.user_id).await?;

    let result = sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if result.rows_affected() == 0 {
        return Err(ApiAppError(AppError::NotFound("Organization not found".to_string())));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Organization deleted successfully" }))))
}

/// GET /orgs/:org_id/members — members with their permission sets
/// GET /orgs/:org_id/members/me — returns the calling user's own membership record.
async fn get_my_membership(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<MemberResponse>>, ApiAppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        org_id: Uuid,
        user_id: Uuid,
        email: String,
        role: String,
        created_at: chrono::DateTime<Utc>,
        permissions: Vec<String>,
    }

    let row = sqlx::query_as::<_, Row>(
        r#"
        SELECT m.id, m.org_id, m.user_id, u.email, m.role::text AS role, m.created_at,
               COALESCE(
                   ARRAY_AGG(p.permission ORDER BY p.permission)
                   FILTER (WHERE p.permission IS NOT NULL),
                   ARRAY[]::text[]
               ) AS permissions
        FROM org_members m
        JOIN users u ON u.id = m.user_id
        LEFT JOIN org_member_permissions p ON p.org_id = m.org_id AND p.user_id = m.user_id
        WHERE m.org_id = $1 AND m.user_id = $2
        GROUP BY m.id, m.org_id, m.user_id, u.email, m.role, m.created_at
        "#,
    )
    .bind(org_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::Forbidden("Not a member of this organization".to_string())))?;

    Ok(Json(ApiResponse::ok(MemberResponse {
        id: row.id,
        org_id: row.org_id,
        user_id: row.user_id,
        email: row.email,
        role: row.role,
        permissions: row.permissions,
        created_at: row.created_at,
    })))
}

async fn list_members(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<ApiResponse<Vec<MemberResponse>>>, ApiAppError> {
    require_member(&state.db, org_id, auth.user_id).await?;

    let per_page = pagination.per_page as i64;
    let offset = ((pagination.page.saturating_sub(1)) as i64) * per_page;

    #[derive(sqlx::FromRow)]
    struct MemberWithPermissions {
        id: Uuid,
        org_id: Uuid,
        user_id: Uuid,
        email: String,
        role: String,
        created_at: chrono::DateTime<Utc>,
        permissions: Vec<String>,
    }

    let members: Vec<MemberWithPermissions> = sqlx::query_as::<_, MemberWithPermissions>(
        r#"
        SELECT m.id, m.org_id, m.user_id, u.email, m.role::text AS role, m.created_at,
               COALESCE(
                   ARRAY_AGG(p.permission ORDER BY p.permission)
                   FILTER (WHERE p.permission IS NOT NULL),
                   ARRAY[]::text[]
               ) AS permissions
        FROM org_members m
        JOIN users u ON u.id = m.user_id
        LEFT JOIN org_member_permissions p ON p.org_id = m.org_id AND p.user_id = m.user_id
        WHERE m.org_id = $1
        GROUP BY m.id, m.org_id, m.user_id, u.email, m.role, m.created_at
        ORDER BY m.created_at ASC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(org_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(
        members.into_iter().map(|m| MemberResponse {
            id: m.id,
            org_id: m.org_id,
            user_id: m.user_id,
            email: m.email,
            role: m.role,
            permissions: m.permissions,
            created_at: m.created_at,
        }).collect(),
    )))
}

/// POST /orgs/:org_id/members/invite
async fn invite_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Json(body): Json<InviteMemberRequest>,
) -> Result<(StatusCode, Json<ApiResponse<InvitationResponse>>), ApiAppError> {
    require_admin(&state.db, org_id, auth.user_id).await?;

    if body.email.is_empty() || !body.email.contains('@') {
        return Err(ApiAppError(AppError::Validation("A valid email address is required".to_string())));
    }
    validate_role(&body.role)?;
    validate_permissions(&body.permissions)?;
    for pa in &body.project_assignments {
        validate_project_permissions(&pa.permissions)?;
    }

    let project_assignments_json = serde_json::to_value(&body.project_assignments)
        .unwrap_or(serde_json::json!([]));

    let token = Uuid::now_v7().to_string();
    let expires_at = Utc::now() + chrono::Duration::days(7);

    let invitation: Invitation = sqlx::query_as::<_, Invitation>(
        r#"
        INSERT INTO invitations
            (id, org_id, email, role, permissions, token, expires_at, created_at, project_assignments)
        VALUES ($1, $2, $3, $4::member_role, $5, $6, $7, NOW(), $8)
        RETURNING id, org_id, email, role::text AS role, permissions, token,
                  expires_at, accepted_at, created_at, project_assignments
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(&body.email)
    .bind(&body.role)
    .bind(&body.permissions)
    .bind(&token)
    .bind(expires_at)
    .bind(&project_assignments_json)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let org: Option<Organization> = sqlx::query_as::<_, Organization>(
        "SELECT id, name, slug, created_at FROM organizations WHERE id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if let Some(org) = org {
        let base_url = state.config.git.frontend_url.clone();
        let smtp_cfg = crate::email::load_smtp_config(&state.db, &state.config.smtp).await;
        if let Err(e) = crate::email::send_invitation_email(
            &smtp_cfg,
            &body.email,
            &org.name,
            &token,
            &base_url,
        )
        .await
        {
            tracing::warn!("Failed to send invitation email: {e}");
        }
    }

    crate::middleware::audit::write_audit_log(
        &state.db,
        Some(auth.user_id),
        "invite_member",
        Some("org"),
        Some(org_id),
        None,
        Some(serde_json::json!({
            "email": body.email,
            "role": body.role,
            "permissions": body.permissions,
            "project_assignments": body.project_assignments.len(),
        })),
    )
    .await;

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(InvitationResponse::from(invitation)))))
}

/// GET /orgs/:org_id/invitations — pending invitations (not accepted, not expired)
async fn list_invitations(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<InvitationResponse>>>, ApiAppError> {
    require_admin(&state.db, org_id, auth.user_id).await?;

    let invitations: Vec<Invitation> = sqlx::query_as::<_, Invitation>(
        r#"
        SELECT id, org_id, email, role::text AS role, permissions, token,
               expires_at, accepted_at, created_at, project_assignments
        FROM invitations
        WHERE org_id = $1
          AND accepted_at IS NULL
          AND expires_at > NOW()
        ORDER BY created_at DESC
        "#,
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(
        invitations.into_iter().map(InvitationResponse::from).collect(),
    )))
}

/// DELETE /orgs/:org_id/invitations/:invitation_id — cancel a pending invitation
async fn cancel_invitation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin(&state.db, org_id, auth.user_id).await?;

    let result = sqlx::query(
        "DELETE FROM invitations WHERE id = $1 AND org_id = $2 AND accepted_at IS NULL",
    )
    .bind(invitation_id)
    .bind(org_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if result.rows_affected() == 0 {
        return Err(ApiAppError(AppError::NotFound(
            "Invitation not found or already accepted".to_string(),
        )));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Invitation cancelled" }))))
}

/// PUT /orgs/:org_id/members/:user_id/role
async fn change_member_role(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, target_user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<ChangeRoleRequest>,
) -> Result<Json<ApiResponse<MemberResponse>>, ApiAppError> {
    let caller = require_admin(&state.db, org_id, auth.user_id).await?;
    validate_role(&body.role)?;

    if body.role == "owner" && caller.role != "owner" {
        return Err(ApiAppError(AppError::Forbidden(
            "Only the owner can assign the owner role".to_string(),
        )));
    }

    let member: OrgMember = sqlx::query_as::<_, OrgMember>(
        r#"
        UPDATE org_members
        SET role = $3::member_role
        WHERE org_id = $1 AND user_id = $2
        RETURNING id, org_id, user_id, role::text AS role, created_at
        "#,
    )
    .bind(org_id)
    .bind(target_user_id)
    .bind(&body.role)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Member not found in this organization".to_string())))?;

    Ok(Json(ApiResponse::ok(MemberResponse::from(member))))
}

/// PUT /orgs/:org_id/members/:user_id/permissions — replace all permissions for a member
async fn set_member_permissions(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, target_user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SetPermissionsRequest>,
) -> Result<Json<ApiResponse<Vec<String>>>, ApiAppError> {
    require_admin(&state.db, org_id, auth.user_id).await?;
    validate_permissions(&body.permissions)?;

    let _ = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM org_members WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(target_user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Member not found".to_string())))?;

    let mut tx = state.db.begin().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    sqlx::query(
        "DELETE FROM org_member_permissions WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(target_user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    for permission in &body.permissions {
        sqlx::query(
            r#"
            INSERT INTO org_member_permissions (id, org_id, user_id, permission, granted_by, created_at)
            VALUES ($1, $2, $3, $4, $5, NOW())
            ON CONFLICT (org_id, user_id, permission) DO NOTHING
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(org_id)
        .bind(target_user_id)
        .bind(permission)
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    tx.commit().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    crate::middleware::audit::write_audit_log(
        &state.db,
        Some(auth.user_id),
        "set_member_permissions",
        Some("org"),
        Some(org_id),
        None,
        Some(serde_json::json!({ "target_user_id": target_user_id, "permissions": body.permissions })),
    )
    .await;

    Ok(Json(ApiResponse::ok(body.permissions)))
}

/// DELETE /orgs/:org_id/members/:user_id
async fn remove_member(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    if auth.user_id != target_user_id {
        require_admin(&state.db, org_id, auth.user_id).await?;
    } else {
        require_member(&state.db, org_id, auth.user_id).await?;
    }

    if auth.user_id != target_user_id {
        let target: Option<OrgMember> = sqlx::query_as::<_, OrgMember>(
            "SELECT id, org_id, user_id, role::text AS role, created_at
             FROM org_members
             WHERE org_id = $1 AND user_id = $2",
        )
        .bind(org_id)
        .bind(target_user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        if let Some(ref m) = target {
            if m.role == "owner" {
                let owner_count: (i64,) = sqlx::query_as(
                    "SELECT COUNT(*) FROM org_members WHERE org_id = $1 AND role::text = 'owner'",
                )
                .bind(org_id)
                .fetch_one(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

                if owner_count.0 <= 1 {
                    return Err(ApiAppError(AppError::Forbidden(
                        "Cannot remove the last owner of an organization".to_string(),
                    )));
                }
            }
        }
    }

    let result = sqlx::query(
        "DELETE FROM org_members WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(target_user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if result.rows_affected() == 0 {
        return Err(ApiAppError(AppError::NotFound("Member not found in this organization".to_string())));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "message": "Member removed successfully" }))))
}

/// POST /orgs/:org_id/invitations/:token/accept — authenticated accept (existing users)
async fn accept_invitation(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, token)): Path<(Uuid, String)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let invitation: Invitation = sqlx::query_as::<_, Invitation>(
        r#"
        SELECT id, org_id, email, role::text AS role, permissions, token,
               expires_at, accepted_at, created_at, project_assignments
        FROM invitations
        WHERE token = $1 AND org_id = $2
        "#,
    )
    .bind(&token)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Invitation not found".to_string())))?;

    if invitation.expires_at < Utc::now() {
        return Err(ApiAppError(AppError::BadRequest("Invitation has expired".to_string())));
    }
    if invitation.accepted_at.is_some() {
        return Err(ApiAppError(AppError::Conflict("Invitation has already been accepted".to_string())));
    }

    let mut tx = state.db.begin().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    sqlx::query(
        r#"
        INSERT INTO org_members (id, org_id, user_id, role, created_at)
        VALUES ($1, $2, $3, $4::member_role, NOW())
        ON CONFLICT (org_id, user_id) DO UPDATE SET role = EXCLUDED.role
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(invitation.org_id)
    .bind(auth.user_id)
    .bind(&invitation.role)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    for permission in &invitation.permissions {
        sqlx::query(
            r#"
            INSERT INTO org_member_permissions (id, org_id, user_id, permission, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            ON CONFLICT (org_id, user_id, permission) DO NOTHING
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(invitation.org_id)
        .bind(auth.user_id)
        .bind(permission)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    apply_project_assignments(&mut tx, invitation.org_id, auth.user_id, &invitation.project_assignments, None).await?;

    sqlx::query("UPDATE invitations SET accepted_at = NOW() WHERE id = $1")
        .bind(invitation.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    tx.commit().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let topic = format!("orgs/{}/members", invitation.org_id);
    let payload = shipyard_common::types::MqttPayload::new("org.member.joined")
        .with_meta(serde_json::json!({
            "org_id": invitation.org_id,
            "user_id": auth.user_id,
            "role": invitation.role,
            "project_count": invitation.project_assignments.as_array().map(|a| a.len()).unwrap_or(0),
        }));
    state.mqtt.publish_status(&topic, &payload).await.ok();

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Invitation accepted successfully",
        "org_id": org_id,
        "role": invitation.role,
    }))))
}

/// GET /orgs/:org_id/audit-logs?cursor=<iso_datetime>&limit=<n>
async fn list_audit_logs(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Query(params): Query<AuditCursorParams>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin(&state.db, org_id, auth.user_id).await?;

    let limit = (params.limit.unwrap_or(50) as i64).clamp(1, 200);
    // Fetch one extra row to detect whether a next page exists.
    let fetch = limit + 1;

    let rows: Vec<shipyard_db::models::AuditLog> = match &params.cursor {
        Some(cursor) => {
            let ts = cursor.parse::<chrono::DateTime<Utc>>().map_err(|_| {
                ApiAppError(AppError::BadRequest("Invalid cursor value".to_string()))
            })?;
            sqlx::query_as::<_, shipyard_db::models::AuditLog>(
                r#"SELECT al.id, al.user_id, al.action, al.resource_type, al.resource_id,
                          al.ip_address, al.metadata, al.created_at
                   FROM audit_logs al
                   WHERE ((al.resource_type = 'org' AND al.resource_id = $1)
                      OR al.user_id IN (SELECT user_id FROM org_members WHERE org_id = $1))
                     AND al.created_at < $2
                   ORDER BY al.created_at DESC
                   LIMIT $3"#,
            )
            .bind(org_id)
            .bind(ts)
            .bind(fetch)
            .fetch_all(&state.db)
            .await
        }
        None => {
            sqlx::query_as::<_, shipyard_db::models::AuditLog>(
                r#"SELECT al.id, al.user_id, al.action, al.resource_type, al.resource_id,
                          al.ip_address, al.metadata, al.created_at
                   FROM audit_logs al
                   WHERE (al.resource_type = 'org' AND al.resource_id = $1)
                      OR al.user_id IN (SELECT user_id FROM org_members WHERE org_id = $1)
                   ORDER BY al.created_at DESC
                   LIMIT $2"#,
            )
            .bind(org_id)
            .bind(fetch)
            .fetch_all(&state.db)
            .await
        }
    }
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let has_more = rows.len() as i64 > limit;
    let rows: Vec<_> = rows.into_iter().take(limit as usize).collect();

    let next_cursor = if has_more {
        rows.last().map(|r| r.created_at.to_rfc3339())
    } else {
        None
    };

    let items: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| serde_json::json!({
            "id": r.id,
            "user_id": r.user_id,
            "action": r.action,
            "resource_type": r.resource_type,
            "resource_id": r.resource_id,
            "ip_address": r.ip_address,
            "metadata": r.metadata,
            "created_at": r.created_at,
        }))
        .collect();

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "items": items,
        "next_cursor": next_cursor,
    }))))
}

#[derive(Debug, Deserialize)]
struct AuditCursorParams {
    cursor: Option<String>,
    limit:  Option<u32>,
}

// ─── Member project assignment handlers ──────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct MemberProjectResponse {
    pub project_id: Uuid,
    pub project_name: String,
    pub project_slug: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SetMemberProjectsRequest {
    pub assignments: Vec<ProjectAssignment>,
}

/// GET /orgs/:org_id/members/:user_id/projects — list project assignments for a member
async fn get_member_projects(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, target_user_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<Vec<MemberProjectResponse>>>, ApiAppError> {
    require_admin(&state.db, org_id, auth.user_id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        project_id: Uuid,
        project_name: String,
        project_slug: String,
        permissions: Vec<String>,
    }

    let rows: Vec<Row> = sqlx::query_as::<_, Row>(
        r#"
        SELECT pm.project_id, p.name AS project_name, p.slug AS project_slug, pm.permissions
        FROM project_members pm
        JOIN projects p ON p.id = pm.project_id
        WHERE pm.org_id = $1 AND pm.user_id = $2
        ORDER BY p.name ASC
        "#,
    )
    .bind(org_id)
    .bind(target_user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(rows.into_iter().map(|r| MemberProjectResponse {
        project_id: r.project_id,
        project_name: r.project_name,
        project_slug: r.project_slug,
        permissions: r.permissions,
    }).collect())))
}

/// PUT /orgs/:org_id/members/:user_id/projects — replace all project assignments for a member
async fn set_member_projects(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, target_user_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<SetMemberProjectsRequest>,
) -> Result<Json<ApiResponse<Vec<MemberProjectResponse>>>, ApiAppError> {
    require_admin(&state.db, org_id, auth.user_id).await?;

    for pa in &body.assignments {
        validate_project_permissions(&pa.permissions)?;
    }

    let mut tx = state.db.begin().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Remove all existing project memberships for this user in this org
    sqlx::query("DELETE FROM project_members WHERE org_id = $1 AND user_id = $2")
        .bind(org_id)
        .bind(target_user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Insert new assignments
    for assignment in &body.assignments {
        sqlx::query(
            r#"
            INSERT INTO project_members (id, project_id, org_id, user_id, permissions, invited_by, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, NOW())
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(assignment.project_id)
        .bind(org_id)
        .bind(target_user_id)
        .bind(&assignment.permissions)
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    tx.commit().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Publish MQTT so the UI can react
    let topic = format!("orgs/{org_id}/members");
    let payload = shipyard_common::types::MqttPayload::new("org.member.projects.updated")
        .with_meta(serde_json::json!({
            "org_id": org_id,
            "user_id": target_user_id,
            "project_count": body.assignments.len(),
        }));
    state.mqtt.publish_status(&topic, &payload).await.ok();

    // Return the updated list by calling the read handler inline
    get_member_projects(auth, State(state), Path((org_id, target_user_id))).await
}

// ─── Public invitation handlers (no auth required) ────────────────────────────

/// GET /invite/:token — public; returns invitation details for the accept-invite page.
async fn get_invite_public(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<ApiResponse<PublicInviteResponse>>, ApiAppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        org_id: Uuid,
        org_name: String,
        email: String,
        role: String,
        permissions: Vec<String>,
        project_assignments: serde_json::Value,
        expires_at: chrono::DateTime<Utc>,
        accepted_at: Option<chrono::DateTime<Utc>>,
    }

    let row: Row = sqlx::query_as::<_, Row>(
        r#"
        SELECT i.org_id, o.name AS org_name, i.email, i.role::text AS role, i.permissions,
               i.project_assignments, i.expires_at, i.accepted_at
        FROM invitations i
        JOIN organizations o ON o.id = i.org_id
        WHERE i.token = $1
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Invitation not found or has expired".to_string())))?;

    Ok(Json(ApiResponse::ok(PublicInviteResponse {
        token,
        org_id: row.org_id,
        org_name: row.org_name,
        email: row.email,
        role: row.role,
        permissions: row.permissions,
        project_assignments: row.project_assignments,
        is_expired: row.expires_at < Utc::now(),
        is_accepted: row.accepted_at.is_some(),
        expires_at: row.expires_at,
    })))
}

/// POST /invite/:token/complete — public; creates a new user account and accepts the invitation.
/// Returns an access token so the user is immediately logged in.
async fn complete_invite(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Json(body): Json<CompleteInviteRequest>,
) -> Result<(HeaderMap, Json<ApiResponse<CompleteInviteResponse>>), ApiAppError> {
    if body.password.len() < 8 {
        return Err(ApiAppError(AppError::Validation(
            "Password must be at least 8 characters".to_string(),
        )));
    }

    let invitation: Invitation = sqlx::query_as::<_, Invitation>(
        r#"
        SELECT id, org_id, email, role::text AS role, permissions, token,
               expires_at, accepted_at, created_at, project_assignments
        FROM invitations
        WHERE token = $1
        "#,
    )
    .bind(&token)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Invitation not found".to_string())))?;

    if invitation.expires_at < Utc::now() {
        return Err(ApiAppError(AppError::BadRequest("This invitation has expired".to_string())));
    }
    if invitation.accepted_at.is_some() {
        return Err(ApiAppError(AppError::Conflict("This invitation has already been accepted".to_string())));
    }

    // Reject if the email is already registered — they should log in instead.
    let existing_user: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM users WHERE email = $1",
    )
    .bind(&invitation.email)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if existing_user.is_some() {
        return Err(ApiAppError(AppError::Conflict(
            "An account with this email already exists. Please log in and accept the invitation from your dashboard.".to_string(),
        )));
    }

    // Hash the password
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(body.password.as_bytes(), &salt)
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to hash password: {e}"))))?
        .to_string();

    let mut tx = state.db.begin().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Create the user
    let user: shipyard_db::models::User = sqlx::query_as::<_, shipyard_db::models::User>(
        "INSERT INTO users (id, email, password_hash, created_at, updated_at)
         VALUES ($1, $2, $3, NOW(), NOW())
         RETURNING id, email, password_hash, created_at, updated_at",
    )
    .bind(Uuid::now_v7())
    .bind(&invitation.email)
    .bind(&password_hash)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Add them to the org
    sqlx::query(
        r#"
        INSERT INTO org_members (id, org_id, user_id, role, created_at)
        VALUES ($1, $2, $3, $4::member_role, NOW())
        ON CONFLICT (org_id, user_id) DO UPDATE SET role = EXCLUDED.role
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(invitation.org_id)
    .bind(user.id)
    .bind(&invitation.role)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Grant org-level permissions
    for permission in &invitation.permissions {
        sqlx::query(
            r#"
            INSERT INTO org_member_permissions (id, org_id, user_id, permission, created_at)
            VALUES ($1, $2, $3, $4, NOW())
            ON CONFLICT (org_id, user_id, permission) DO NOTHING
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(invitation.org_id)
        .bind(user.id)
        .bind(permission)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    // Apply project assignments
    apply_project_assignments(&mut tx, invitation.org_id, user.id, &invitation.project_assignments, None).await?;

    // Mark invitation as accepted
    sqlx::query("UPDATE invitations SET accepted_at = NOW() WHERE id = $1")
        .bind(invitation.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    tx.commit().await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let topic = format!("orgs/{}/members", invitation.org_id);
    let payload = shipyard_common::types::MqttPayload::new("org.member.joined")
        .with_meta(serde_json::json!({
            "org_id": invitation.org_id,
            "user_id": user.id,
            "role": invitation.role,
            "project_count": invitation.project_assignments.as_array().map(|a| a.len()).unwrap_or(0),
        }));
    state.mqtt.publish_status(&topic, &payload).await.ok();

    // Issue tokens so the user is immediately logged in
    let access_token = create_access_token(
        user.id,
        &user.email,
        &state.config.auth.jwt_secret,
        state.config.auth.access_token_expiry,
    )
    .map_err(ApiAppError)?;

    let (refresh_token, jti) = create_refresh_token(
        user.id,
        &user.email,
        &state.config.auth.jwt_secret,
        state.config.auth.refresh_token_expiry,
    )
    .map_err(ApiAppError)?;

    session::save_session(
        &state.redis,
        &jti,
        &session::SessionData {
            user_id: user.id,
            email: user.email.clone(),
            created_at: Utc::now(),
        },
        state.config.auth.refresh_token_expiry,
    ).await;

    let mut headers = HeaderMap::new();
    headers.insert(
        SET_COOKIE,
        set_refresh_cookie(&refresh_token, state.config.auth.refresh_token_expiry),
    );

    Ok((
        headers,
        Json(ApiResponse::ok(CompleteInviteResponse {
            access_token,
            user_id: user.id,
            email: user.email,
        })),
    ))
}
