use sqlx::PgPool;
use uuid::Uuid;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

async fn pool() -> PgPool {
    let pool = shipyard_db::init_pool(&test_database_url(), 5).await.expect("connect");
    shipyard_db::run_migrations(&pool).await.expect("migrate");
    pool
}

async fn seed_running_sandbox(pool: &PgPool, heartbeat_age_secs: i64) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug, plan_id) VALUES ($1, $2, $3, (SELECT id FROM plans WHERE name = 'free'))")
        .bind(org_id).bind(format!("org-{suffix}")).bind(format!("org-{suffix}")).execute(pool).await.unwrap();
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, org_id, name, slug, directory_path) VALUES ($1, $2, $3, $4, $5)")
        .bind(project_id).bind(org_id).bind(format!("p-{suffix}")).bind(format!("p-{suffix}")).bind(format!("/tmp/{suffix}")).execute(pool).await.unwrap();
    let service_id = Uuid::new_v4();
    sqlx::query("INSERT INTO services (id, project_id, name, slug, type, status, replicas, ports) VALUES ($1, $2, $3, $4, 'sandbox_app', 'running', 0, '[]'::jsonb)")
        .bind(service_id).bind(project_id).bind(format!("a-{suffix}")).bind(format!("a-{suffix}")).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO sandbox_instances (service_id, status, last_heartbeat_at) VALUES ($1, 'running', NOW() - ($2 || ' seconds')::interval)")
        .bind(service_id).bind(heartbeat_age_secs.to_string()).execute(pool).await.unwrap();
    service_id
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn finds_sandboxes_past_idle_timeout() {
    let pool = pool().await;
    let stale_id = seed_running_sandbox(&pool, 2000).await; // 33 min ago
    let fresh_id = seed_running_sandbox(&pool, 60).await;   // 1 min ago

    let stale = shipyard_api::sandbox_runtime::reaper::find_stale_sandboxes(&pool, 1200).await.unwrap();
    assert!(stale.contains(&stale_id), "sandbox idle for 2000s should be found stale at a 1200s timeout");
    assert!(!stale.contains(&fresh_id), "sandbox idle for 60s should not be found stale at a 1200s timeout");
}
