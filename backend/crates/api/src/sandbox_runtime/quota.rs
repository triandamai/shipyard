use sqlx::PgPool;
use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

/// Checks the org's dedicated sandbox quota (separate from production
/// resource limits — see plans.max_concurrent_sandboxes). Returns `Ok(Ok(()))`
/// when under quota, `Ok(Err(message))` with a user-facing message when not.
pub async fn check_quota(db: &PgPool, org_id: Uuid) -> AppResult<Result<(), String>> {
    // LEFT JOIN + fetch_optional: `organizations.plan_id` is nullable
    // (`ON DELETE SET NULL`), and this path is reachable unauthenticated via
    // the public cold-start route — a planless org must get an actionable
    // denial, not a 500 from `RowNotFound`.
    let max_concurrent: Option<i32> = sqlx::query_scalar(
        "SELECT p.max_concurrent_sandboxes
         FROM organizations o LEFT JOIN plans p ON p.id = o.plan_id
         WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .flatten();

    let Some(max_concurrent) = max_concurrent else {
        return Ok(Err(
            "This organization has no active plan — sandboxes are unavailable.".to_string(),
        ));
    };

    // Counts 'starting' as well as 'running': a sandbox spends real time
    // (probe + install) in 'starting', so a burst of concurrent starts would
    // otherwise all pass a running-only check and together exceed the cap.
    let running_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sandbox_instances si
         JOIN services s ON s.id = si.service_id
         JOIN projects p ON p.id = s.project_id
         WHERE p.org_id = $1 AND si.status IN ('running', 'starting')",
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
