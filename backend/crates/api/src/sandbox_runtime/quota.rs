use sqlx::PgPool;
use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

/// Checks the org's dedicated sandbox quota (separate from production
/// resource limits — see plans.max_concurrent_sandboxes). Returns `Ok(Ok(()))`
/// when under quota, `Ok(Err(message))` with a user-facing message when not.
pub async fn check_quota(db: &PgPool, org_id: Uuid) -> AppResult<Result<(), String>> {
    let max_concurrent: i32 = sqlx::query_scalar(
        "SELECT p.max_concurrent_sandboxes
         FROM organizations o JOIN plans p ON p.id = o.plan_id
         WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let running_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sandbox_instances si
         JOIN services s ON s.id = si.service_id
         JOIN projects p ON p.id = s.project_id
         WHERE p.org_id = $1 AND si.status = 'running'",
    )
    .bind(org_id)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    if running_count >= max_concurrent as i64 {
        return Ok(Err(format!(
            "This organization has reached its limit of {max_concurrent} concurrent sandbox(es). Stop another sandbox or upgrade your plan."
        )));
    }
    Ok(Ok(()))
}
