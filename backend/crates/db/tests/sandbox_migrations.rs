use sqlx::PgPool;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

async fn pool() -> PgPool {
    let pool = shipyard_db::init_pool(&test_database_url(), 5)
        .await
        .expect("failed to connect to TEST_DATABASE_URL — is Postgres running?");
    shipyard_db::run_migrations(&pool).await.expect("migrations failed to apply");
    pool
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn sandbox_tables_and_quota_columns_exist() {
    let pool = pool().await;

    let quota: (i32, f64, f64) = sqlx::query_as(
        "SELECT max_concurrent_sandboxes, sandbox_cpu_cores, sandbox_memory_gb FROM plans WHERE name = 'free'",
    )
    .fetch_one(&pool)
    .await
    .expect("free plan should have sandbox quota columns populated");
    assert_eq!(quota, (1, 0.5, 1.0));

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sandbox_app_configs")
        .fetch_one(&pool)
        .await
        .expect("sandbox_app_configs table should exist and be queryable");
    assert!(count >= 0);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sandbox_instances")
        .fetch_one(&pool)
        .await
        .expect("sandbox_instances table should exist and be queryable");
    assert!(count >= 0);
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn sandbox_migrations_rerun_cleanly() {
    let pool = pool().await;
    shipyard_db::run_migrations(&pool).await.expect("re-running migrations should be a no-op, not an error");
}
