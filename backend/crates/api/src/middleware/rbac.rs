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
        _ => Err(AppError::Forbidden(
            "You need admin or owner access to perform this action.".to_string()
        )),
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
        Err(AppError::Forbidden(
            "Only the organization owner can perform this action.".to_string()
        ))
    }
}

/// Require membership in the org that owns a project.
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
        "Project '{}' not found", project_id
    )))?;

    require_org_member(db, user_id, org_id)
        .await
        .map(|_| ())
}

/// Validate a `shipyard:` permission string structurally.
///
/// Valid forms:
///   `shipyard:<uuid>:<resource>:<action>`              (org-level)
///   `shipyard:<uuid>:<uuid>:<resource>:<action>`       (project-level)
pub fn validate_permission_string(perm: &str) -> Result<(), AppError> {
    let parts: Vec<&str> = perm.split(':').collect();
    if parts.len() < 4 || parts[0] != "shipyard" {
        return Err(AppError::Validation(format!(
            "Invalid permission format: '{perm}'. \
             Expected shipyard:<org_id>:<resource>:<action>"
        )));
    }
    // Second segment must be a valid UUID (org_id)
    parts[1].parse::<Uuid>().map_err(|_| AppError::Validation(format!(
        "Invalid org_id in permission '{perm}'"
    )))?;
    // If five segments, the third is a project UUID
    if parts.len() == 5 {
        parts[2].parse::<Uuid>().map_err(|_| AppError::Validation(format!(
            "Invalid project_id in permission '{perm}'"
        )))?;
    }
    Ok(())
}

/// Check if a user has a specific fine-grained permission in an org.
///
/// Owners and admins bypass all checks.
/// For members/viewers, checks both:
///   1. `org_member_permissions` (org-level shipyard: strings)
///   2. `project_members.permissions` (project-scoped shipyard: strings)
pub async fn require_permission(
    db: &PgPool,
    user_id: Uuid,
    org_id: Uuid,
    permission: &str,
) -> Result<(), AppError> {
    // Superadmin bypasses all org-level checks
    let is_super: Option<(bool,)> = sqlx::query_as(
        "SELECT is_superadmin FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    if matches!(is_super, Some((true,))) {
        return Ok(());
    }

    let role = get_user_org_role(db, user_id, org_id).await
        .ok_or_else(|| AppError::Forbidden(format!(
            "User '{}' is not a member of organization '{}'", user_id, org_id
        )))?;

    if role == "owner" || role == "admin" {
        return Ok(());
    }

    // Check org-level permissions
    let in_org: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
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

    if in_org.is_some() {
        return Ok(());
    }

    // Check project-level permissions (stored as full shipyard: strings in project_members)
    let in_project: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM project_members
         WHERE org_id = $1 AND user_id = $2 AND $3 = ANY(permissions)",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(permission)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    if in_project.is_some() {
        Ok(())
    } else {
        Err(AppError::Forbidden(
            "You don't have permission to perform this action.".to_string()
        ))
    }
}

/// Require a specific permission resolved via a project's org.
/// `perm_suffix` is the resource:action suffix, e.g. `"service:write"`.
/// The full `shipyard:<org_id>:<project_id>:<perm_suffix>` string is built internally.
pub async fn require_project_permission(
    db: &PgPool,
    user_id: Uuid,
    project_id: Uuid,
    perm_suffix: &str,
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

    let full_perm = format!("shipyard:{org_id}:{project_id}:{perm_suffix}");
    require_permission(db, user_id, org_id, &full_perm).await
}

/// Require a specific permission resolved via a service's project/org.
/// `perm_suffix` is the resource:action suffix, e.g. `"service:deploy"`.
/// The full `shipyard:<org_id>:<project_id>:<perm_suffix>` string is built internally.
pub async fn require_service_permission(
    db: &PgPool,
    user_id: Uuid,
    service_id: Uuid,
    perm_suffix: &str,
) -> Result<(), AppError> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT p.org_id, s.project_id \
         FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let (org_id, project_id) = row.ok_or_else(|| AppError::NotFound(format!(
        "Service '{}' not found", service_id
    )))?;

    let full_perm = format!("shipyard:{org_id}:{project_id}:{perm_suffix}");
    require_permission(db, user_id, org_id, &full_perm).await
}

/// Check if a user has access to a service (via project → org membership).
pub async fn require_service_access(
    db: &PgPool,
    user_id: Uuid,
    service_id: Uuid,
) -> Result<(), AppError> {
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
        "Service '{}' not found", service_id
    )))?;

    require_org_member(db, user_id, org_id)
        .await
        .map(|_| ())
}
