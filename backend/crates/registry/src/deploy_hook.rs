//! Bridges a registry push to the deployment queue.
//!
//! A `docker push` over HTTP, or an explicit deploy webhook, only needs to
//! insert a `deployments` row with `status = 'queued'`; the API's deployment
//! scheduler polls for those and runs them. This lives in the registry crate
//! (rather than `engine`) to avoid a crate cycle — `engine` already depends on
//! `registry`.

use sqlx::PgPool;
use uuid::Uuid;

/// Queue a deployment for `service_id` unless one is already queued or running.
///
/// Returns the new deployment id, or `None` when a deployment is already in
/// flight — repeated pushes then coalesce into a single deploy instead of
/// stacking up.
pub async fn enqueue_service_deploy(
    pool: &PgPool,
    service_id: Uuid,
    triggered_by: &str,
    source_ref: &str,
) -> sqlx::Result<Option<Uuid>> {
    let created: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         SELECT $1, $2, $3, $4, 'queued'::deployment_status, NOW()
         WHERE NOT EXISTS (
             SELECT 1 FROM deployments
             WHERE service_id = $2
               AND status IN ('queued'::deployment_status, 'running'::deployment_status)
         )
         RETURNING id",
    )
    .bind(Uuid::now_v7())
    .bind(service_id)
    .bind(triggered_by)
    .bind(source_ref)
    .fetch_optional(pool)
    .await?;

    Ok(created.map(|(id,)| id))
}

/// Queue deploys for every service whose artifact source matches this push
/// (`namespace_id` + `repo` + `tag`) and has `auto_deploy_on_push` enabled.
/// Returns the deployment ids actually created.
pub async fn enqueue_auto_deploys_for_push(
    pool: &PgPool,
    namespace_id: Uuid,
    repo: &str,
    tag: &str,
) -> sqlx::Result<Vec<Uuid>> {
    let services: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT service_id FROM service_artifact_sources
         WHERE namespace_id = $1 AND repo = $2 AND tag = $3 AND auto_deploy_on_push = TRUE",
    )
    .bind(namespace_id)
    .bind(repo)
    .bind(tag)
    .fetch_all(pool)
    .await?;

    let mut created = Vec::new();
    for (service_id,) in services {
        if let Some(id) =
            enqueue_service_deploy(pool, service_id, "registry-push", "registry-push").await?
        {
            created.push(id);
        }
    }
    Ok(created)
}
