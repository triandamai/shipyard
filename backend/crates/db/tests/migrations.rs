//! DB integration test: runs the full migration set against a real Postgres
//! and sanity-checks the resulting schema. Requires a reachable Postgres —
//! marked `#[ignore]` so `cargo test` stays DB-free by default.
//!
//! Run explicitly with a local dev DB:
//!   docker compose -f infra/docker-compose.dev.yml up -d postgres
//!   cargo test -p shipyard-db -- --ignored
//!
//! CI provides Postgres via a service container and runs with `--include-ignored`.

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn migrations_run_cleanly_and_produce_a_usable_schema() {
    let pool = shipyard_db::init_pool(&test_database_url(), 5)
        .await
        .expect("failed to connect to TEST_DATABASE_URL — is Postgres running?");

    shipyard_db::run_migrations(&pool)
        .await
        .expect("migrations failed to apply");

    // Sanity check: `services` is the root table every resource type hangs off of.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM services")
        .fetch_one(&pool)
        .await
        .expect("services table should exist and be queryable after migrations");
    assert!(count >= 0);

    // Re-running migrations against the same DB must be idempotent (no error).
    shipyard_db::run_migrations(&pool)
        .await
        .expect("re-running migrations should be a no-op, not an error");
}
