//! Integration tests for `deploy_hook` — the bridge from a registry push to the
//! deployment queue. Requires a reachable Postgres (see `TEST_DATABASE_URL`);
//! `#[ignore]` so `cargo test` stays DB-free by default. CI runs it with
//! `--include-ignored` against its Postgres service container.

use shipyard_registry::deploy_hook::{enqueue_auto_deploys_for_push, enqueue_service_deploy};
use sqlx::PgPool;
use uuid::Uuid;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

async fn pool() -> PgPool {
    let pool = shipyard_db::init_pool(&test_database_url(), 5)
        .await
        .expect("connect to TEST_DATABASE_URL — is Postgres running?");
    shipyard_db::run_migrations(&pool)
        .await
        .expect("migrations apply");
    pool
}

/// Create org → project → service → registry namespace, all with random slugs
/// so tests don't collide on a shared database. Returns `(service_id, namespace_id)`.
async fn seed_service(pool: &PgPool) -> (Uuid, Uuid) {
    let suffix = Uuid::new_v4().simple().to_string();
    let org_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let service_id = Uuid::new_v4();
    let namespace_id = Uuid::new_v4();

    sqlx::query("INSERT INTO organizations (id, name, slug) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(format!("org-{suffix}"))
        .bind(format!("org-{suffix}"))
        .execute(pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO projects (id, org_id, name, slug, directory_path) VALUES ($1, $2, $3, $4, '')",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(format!("proj-{suffix}"))
    .bind(format!("proj-{suffix}"))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO services (id, project_id, name, slug, type) VALUES ($1, $2, $3, $4, 'docker')",
    )
    .bind(service_id)
    .bind(project_id)
    .bind(format!("svc-{suffix}"))
    .bind(format!("svc-{suffix}"))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO registry_namespaces (id, org_id, project_id, slug) VALUES ($1, $2, $3, $4)",
    )
    .bind(namespace_id)
    .bind(org_id)
    .bind(project_id)
    .bind(format!("org-{suffix}/proj-{suffix}"))
    .execute(pool)
    .await
    .unwrap();

    (service_id, namespace_id)
}

async fn set_artifact_source(
    pool: &PgPool,
    service_id: Uuid,
    namespace_id: Uuid,
    repo: &str,
    tag: &str,
    auto_deploy: bool,
) {
    sqlx::query(
        "INSERT INTO service_artifact_sources
             (service_id, namespace_id, repo, tag, auto_deploy_on_push)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(service_id)
    .bind(namespace_id)
    .bind(repo)
    .bind(tag)
    .bind(auto_deploy)
    .execute(pool)
    .await
    .unwrap();
}

async fn deployment_rows(pool: &PgPool, service_id: Uuid) -> Vec<(String, String)> {
    sqlx::query_as::<_, (String, String)>(
        "SELECT status::text, source_ref FROM deployments WHERE service_id = $1 ORDER BY created_at",
    )
    .bind(service_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn enqueue_service_deploy_creates_one_queued_row() {
    let pool = pool().await;
    let (service_id, _ns) = seed_service(&pool).await;

    let id = enqueue_service_deploy(&pool, service_id, "webhook", "manual-ci")
        .await
        .unwrap();

    assert!(id.is_some(), "a deployment id should be returned");
    let rows = deployment_rows(&pool, service_id).await;
    assert_eq!(rows, vec![("queued".to_string(), "manual-ci".to_string())]);
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn enqueue_service_deploy_coalesces_while_one_is_in_flight() {
    let pool = pool().await;
    let (service_id, _ns) = seed_service(&pool).await;

    // A deployment is already running for this service.
    sqlx::query(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         VALUES ($1, $2, 'user', 'first', 'running'::deployment_status, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(service_id)
    .execute(&pool)
    .await
    .unwrap();

    let id = enqueue_service_deploy(&pool, service_id, "webhook", "second")
        .await
        .unwrap();

    assert!(id.is_none(), "should not queue a second deploy while one is in flight");
    let rows = deployment_rows(&pool, service_id).await;
    assert_eq!(rows.len(), 1, "still just the running deployment");
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn auto_deploy_skips_sources_without_the_flag() {
    let pool = pool().await;
    let (service_id, namespace_id) = seed_service(&pool).await;
    set_artifact_source(&pool, service_id, namespace_id, "backend", "latest", false).await;

    let created = enqueue_auto_deploys_for_push(&pool, namespace_id, "backend", "latest")
        .await
        .unwrap();

    assert!(created.is_empty());
    assert!(deployment_rows(&pool, service_id).await.is_empty());
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn auto_deploy_enqueues_flagged_matching_source_only() {
    let pool = pool().await;
    let (service_id, namespace_id) = seed_service(&pool).await;
    set_artifact_source(&pool, service_id, namespace_id, "backend", "latest", true).await;

    // Non-matching tag on the same namespace/repo must not trigger this service.
    let created_other = enqueue_auto_deploys_for_push(&pool, namespace_id, "backend", "v1.2.3")
        .await
        .unwrap();
    assert!(created_other.is_empty(), "tag mismatch must not deploy");

    let created = enqueue_auto_deploys_for_push(&pool, namespace_id, "backend", "latest")
        .await
        .unwrap();

    assert_eq!(created.len(), 1);
    let rows = deployment_rows(&pool, service_id).await;
    assert_eq!(
        rows,
        vec![("queued".to_string(), "registry-push".to_string())]
    );
}
