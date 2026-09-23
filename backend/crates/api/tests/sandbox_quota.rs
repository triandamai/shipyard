use sqlx::PgPool;
use uuid::Uuid;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

async fn pool() -> PgPool {
    let pool = shipyard_db::init_pool(&test_database_url(), 5).await.expect("connect to TEST_DATABASE_URL");
    shipyard_db::run_migrations(&pool).await.expect("migrations apply");
    pool
}

/// Seeds an org on the free plan (max_concurrent_sandboxes = 1) with one
/// project, returning the org_id.
async fn seed_free_org(pool: &PgPool) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug, plan_id) VALUES ($1, $2, $3, (SELECT id FROM plans WHERE name = 'free'))")
        .bind(org_id)
        .bind(format!("Test Org {suffix}"))
        .bind(format!("test-org-{suffix}"))
        .execute(pool)
        .await
        .expect("insert organization");
    org_id
}

async fn seed_sandbox_service(pool: &PgPool, org_id: Uuid, status: &str) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, org_id, name, slug, directory_path) VALUES ($1, $2, $3, $4, $5)")
        .bind(project_id).bind(org_id).bind(format!("proj-{suffix}")).bind(format!("proj-{suffix}")).bind(format!("/tmp/{suffix}"))
        .execute(pool).await.expect("insert project");

    let service_id = Uuid::new_v4();
    sqlx::query("INSERT INTO services (id, project_id, name, slug, type, status, replicas, ports) VALUES ($1, $2, $3, $4, 'sandbox_app', 'running', 0, '[]'::jsonb)")
        .bind(service_id).bind(project_id).bind(format!("app-{suffix}")).bind(format!("app-{suffix}"))
        .execute(pool).await.expect("insert service");

    sqlx::query("INSERT INTO sandbox_instances (service_id, status) VALUES ($1, $2)")
        .bind(service_id).bind(status)
        .execute(pool).await.expect("insert sandbox_instances");

    service_id
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn free_org_under_quota_with_zero_running_sandboxes() {
    let pool = pool().await;
    let org_id = seed_free_org(&pool).await;
    let result = shipyard_api::sandbox_runtime::quota::check_quota(&pool, org_id).await.unwrap();
    assert!(result.is_ok(), "org with 0 running sandboxes should be under its quota of 1");
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn free_org_over_quota_with_one_running_sandbox() {
    let pool = pool().await;
    let org_id = seed_free_org(&pool).await;
    seed_sandbox_service(&pool, org_id, "running").await;

    let result = shipyard_api::sandbox_runtime::quota::check_quota(&pool, org_id).await.unwrap();
    assert!(result.is_err(), "org already at its quota of 1 running sandbox should be over quota");
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn stopped_sandboxes_do_not_count_against_quota() {
    let pool = pool().await;
    let org_id = seed_free_org(&pool).await;
    seed_sandbox_service(&pool, org_id, "stopped").await;

    let result = shipyard_api::sandbox_runtime::quota::check_quota(&pool, org_id).await.unwrap();
    assert!(result.is_ok(), "a stopped sandbox should not count against the concurrent quota");
}
