use sqlx::PgPool;
use uuid::Uuid;

use shipyard_common::error::AppError;

// ─── RBAC helper functions ────────────────────────────────────────────────────

/// Get the user's role in an org. Returns None if not a member.
pub async fn get_user_org_role(
    db: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
) -> Option<String> {
    sqlx::query_as::<_, (String,)>(
        "SELECT role::text FROM org_members WHERE user_id = $1 AND org_id = $2",
    )
    .bind(user_id)
    .bind(org_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .map(|(role,)| role)
}

/// Require membership. Returns AppError::Forbidden if not a member.
pub async fn require_org_member(
    db: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<String, AppError> {
    get_user_org_role(db, user_id, org_id)
        .await
        .ok_or_else(|| AppError::Forbidden(format!(
            "User '{}' is not a member of organization '{}'",
            user_id, org_id
        )))
}

/// Require admin or owner role.
#[allow(dead_code)]
pub async fn require_org_admin(
    db: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<(), AppError> {
    let role = require_org_member(db, user_id, org_id).await?;
    match role.as_str() {
        "owner" | "admin" => Ok(()),
        _ => Err(AppError::Forbidden(format!(
            "User '{}' does not have admin access to organization '{}'",
            user_id, org_id
        ))),
    }
}

/// Require owner role only.
#[allow(dead_code)]
pub async fn require_org_owner(
    db: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<(), AppError> {
    let role = require_org_member(db, user_id, org_id).await?;
    if role.as_str() == "owner" {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "User '{}' is not the owner of organization '{}'",
            user_id, org_id
        )))
    }
}

/// Check if a user has access to a project (is member of the project's org).
pub async fn require_project_access(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
) -> Result<(), AppError> {
    let org_id: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let (org_id,) = org_id.ok_or_else(|| AppError::NotFound(format!(
        "Project '{}' not found",
        project_id
    )))?;

    require_org_member(db, user_id, org_id)
        .await
        .map(|_| ())
}

/// Check if a user has a specific fine-grained permission in an org.
/// Owners and admins bypass the check automatically.
pub async fn require_permission(
    db: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
    permission: &str,
) -> Result<(), AppError> {
    let role = get_user_org_role(db, user_id, org_id).await
        .ok_or_else(|| AppError::Forbidden(format!(
            "User '{}' is not a member of organization '{}'",
            user_id, org_id
        )))?;

    if role == "owner" || role == "admin" {
        return Ok(());
    }

    let has_perm: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_member_permissions
         WHERE org_id = $1 AND user_id = $2 AND permission = $3",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(permission)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    if has_perm.is_some() {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "User '{}' does not have permission '{}'",
            user_id, permission
        )))
    }
}

/// Require a specific permission resolved via a project's org.
/// Owners and admins bypass the check.
pub async fn require_project_permission(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    permission: &str,
) -> Result<(), AppError> {
    let org_id: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
        "SELECT org_id FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let (org_id,) = org_id.ok_or_else(|| AppError::NotFound(format!(
        "Project '{}' not found", project_id
    )))?;

    require_permission(db, user_id, org_id, permission).await
}

/// Require a specific permission resolved via a service's project/org.
/// Owners and admins bypass the check.
pub async fn require_service_permission(
    db: &PgPool,
    user_id: Uuid,
    service_id: Uuid,
    permission: &str,
) -> Result<(), AppError> {
    let org_id: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
        "SELECT p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let (org_id,) = org_id.ok_or_else(|| AppError::NotFound(format!(
        "Service '{}' not found", service_id
    )))?;

    require_permission(db, user_id, org_id, permission).await
}

/// Check if a user has access to a service (via project → org membership).
pub async fn require_service_access(
    db: &PgPool,
    user_id: Uuid,
    service_id: Uuid,
) -> Result<(), AppError> {
    // Resolve service → project → org in one query
    let org_id: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
        "SELECT p.org_id
         FROM services s
         JOIN projects p ON p.id = s.project_id
         WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let (org_id,) = org_id.ok_or_else(|| AppError::NotFound(format!(
        "Service '{}' not found",
        service_id
    )))?;

    require_org_member(db, user_id, org_id)
        .await
        .map(|_| ())
}
