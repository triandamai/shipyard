use std::sync::Arc;
use std::time::Duration;
use sqlx::PgPool;
use uuid::Uuid;
use shipyard_common::error::{AppError, AppResult};

use crate::AppState;
use super::manager;

/// Finds sandboxes whose last heartbeat is older than `idle_timeout_secs`.
/// Idle timeout is the source of truth for "is this sandbox actually in
/// use" (see spec Decisions) — explicit stop and tab-close signals are
/// best-effort and can be missed, so this is the reliable backstop.
pub async fn find_stale_sandboxes(db: &PgPool, idle_timeout_secs: u64) -> AppResult<Vec<Uuid>> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT service_id FROM sandbox_instances
         WHERE status = 'running'
           AND last_heartbeat_at IS NOT NULL
           AND last_heartbeat_at < NOW() - ($1 || ' seconds')::interval",
    )
    .bind(idle_timeout_secs.to_string())
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

async fn tick(state: &AppState) -> AppResult<()> {
    let stale = find_stale_sandboxes(&state.db, state.config.sandbox.idle_timeout_secs).await?;
    for service_id in stale {
        if let Err(e) = manager::stop_sandbox(state, service_id).await {
            tracing::warn!(service_id = %service_id, "idle reaper: failed to stop sandbox: {e}");
        } else {
            tracing::info!(service_id = %service_id, "idle reaper: stopped idle sandbox");
        }
    }
    Ok(())
}

/// Runs on `state.config.sandbox.reaper_interval_secs` (default 60s). Mirrors
/// `edge_functions::runtime_worker::run`'s tokio::time::interval shape.
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(state.config.sandbox.reaper_interval_secs));
    loop {
        interval.tick().await;
        if let Err(e) = tick(&state).await {
            tracing::error!("SandboxReaper error: {e}");
        }
    }
}
