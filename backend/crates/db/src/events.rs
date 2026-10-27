//! Platform events: a durable record of what happened to an org's resources,
//! fanned out to subscribed webhook endpoints.
//!
//! `emit` writes the event and queues one delivery per matching endpoint in a
//! single statement; the API's webhook dispatcher sends them with retries.
//! Emitting never fails the caller: errors are logged and swallowed, because
//! an event is a side effect of work that already happened.
//!
//! Event types are `<resource>.<what happened>`, e.g. `deployment.succeeded`,
//! `domain.attached`, `service.crashed`. Endpoints subscribe with exact types,
//! prefixes (`deployment.*`) or `*`.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

/// Where an event happened. `project_id`/`service_id` are optional because
/// org-level events (org created, dev database) have neither.
#[derive(Debug, Clone, Copy)]
pub struct Scope {
    pub org_id: Uuid,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
}

impl Scope {
    pub fn org(org_id: Uuid) -> Self {
        Self {
            org_id,
            project_id: None,
            service_id: None,
        }
    }
    pub fn project(org_id: Uuid, project_id: Uuid) -> Self {
        Self {
            org_id,
            project_id: Some(project_id),
            service_id: None,
        }
    }
    pub fn service(org_id: Uuid, project_id: Uuid, service_id: Uuid) -> Self {
        Self {
            org_id,
            project_id: Some(project_id),
            service_id: Some(service_id),
        }
    }
}

/// Record an event and queue deliveries. Returns the event id, or `None` if
/// it couldn't be written.
pub async fn emit(db: &PgPool, scope: Scope, event_type: &str, data: Value) -> Option<Uuid> {
    let id = Uuid::now_v7();
    let result = sqlx::query(
        r#"
        WITH ev AS (
            INSERT INTO platform_events (id, org_id, org_external_id, project_id, service_id, type, data)
            SELECT $1, $2, (SELECT external_id FROM organizations WHERE id = $2), $3, $4, $5, $6
            RETURNING id, org_id, type
        )
        INSERT INTO webhook_deliveries (endpoint_id, event_id)
        SELECT we.id, ev.id
        FROM ev
        JOIN organizations o ON o.id = ev.org_id
        JOIN webhook_endpoints we
          ON we.enabled
         AND (we.org_id = ev.org_id
              OR (we.integration_id IS NOT NULL AND we.integration_id = o.integration_id))
        WHERE EXISTS (
            SELECT 1 FROM unnest(we.events) AS p
            WHERE p = '*'
               OR p = ev.type
               OR (right(p, 2) = '.*' AND starts_with(ev.type, left(p, length(p) - 1)))
        )
        "#,
    )
    .bind(id)
    .bind(scope.org_id)
    .bind(scope.project_id)
    .bind(scope.service_id)
    .bind(event_type)
    .bind(&data)
    .execute(db)
    .await;

    match result {
        Ok(_) => Some(id),
        Err(e) => {
            tracing::warn!(event_type, org_id = %scope.org_id, "failed to record event: {e}");
            None
        }
    }
}

/// Look up the org of a project, for callers that only hold a project id.
pub async fn project_scope(db: &PgPool, project_id: Uuid) -> Option<Scope> {
    sqlx::query_scalar::<_, Uuid>("SELECT org_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .map(|org_id| Scope::project(org_id, project_id))
}

/// `emit` for callers that only hold a project id.
pub async fn emit_for_project(db: &PgPool, project_id: Uuid, event_type: &str, data: Value) {
    if let Some(scope) = project_scope(db, project_id).await {
        emit(db, scope, event_type, data).await;
    }
}

/// Look up the org and project of a service, for callers that only hold a
/// service id.
pub async fn service_scope(db: &PgPool, service_id: Uuid) -> Option<Scope> {
    sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT p.org_id, p.id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .map(|(org_id, project_id)| Scope::service(org_id, project_id, service_id))
}

/// `emit` for callers that only hold a service id.
pub async fn emit_for_service(db: &PgPool, service_id: Uuid, event_type: &str, data: Value) {
    if let Some(scope) = service_scope(db, service_id).await {
        emit(db, scope, event_type, data).await;
    }
}

/// Whether `event_type` matches a subscription pattern. Mirrors the SQL in
/// [`emit`]; used to validate patterns and in tests.
pub fn pattern_matches(pattern: &str, event_type: &str) -> bool {
    if pattern == "*" || pattern == event_type {
        return true;
    }
    match pattern.strip_suffix(".*") {
        Some(prefix) => event_type.starts_with(&format!("{prefix}.")),
        None => false,
    }
}

/// Every event type Shipyard emits, for docs and pattern validation.
pub const EVENT_TYPES: &[&str] = &[
    "org.created",
    "org.deleted",
    "app.created",
    "app.terminated",
    "project.created",
    "project.duplicated",
    "project.deleted",
    "service.created",
    "service.deleted",
    "service.env_changed",
    "service.crashed",
    "service.resource_alert",
    "service.error_rate_alert",
    "deployment.started",
    "deployment.step_started",
    "deployment.step_succeeded",
    "deployment.step_failed",
    "deployment.succeeded",
    "deployment.failed",
    "domain.attached",
    "domain.removed",
    "volume.created",
    "volume.attached",
    "volume.deleted",
    "network.created",
    "network.attached",
    "network.detached",
    "network.deleted",
    "database.provisioned",
    "dev_database.created",
    "dev_database.deleted",
    "secret.created",
    "secret.updated",
    "secret.deleted",
    "publish.started",
    "publish.succeeded",
    "publish.failed",
];

/// Whether a subscription pattern can match at least one known event type.
pub fn pattern_is_known(pattern: &str) -> bool {
    EVENT_TYPES.iter().any(|t| pattern_matches(pattern, t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns() {
        assert!(pattern_matches("*", "deployment.failed"));
        assert!(pattern_matches("deployment.failed", "deployment.failed"));
        assert!(pattern_matches("deployment.*", "deployment.step_failed"));
        assert!(!pattern_matches("deployment.*", "deploymentx.failed"));
        assert!(!pattern_matches("domain.attached", "domain.removed"));
        assert!(pattern_is_known("volume.*"));
        assert!(!pattern_is_known("nope.*"));
    }
}
