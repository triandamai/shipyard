pub mod models;
pub mod redis;
pub mod repo;

/// Re-export the database pool type for convenience.
pub type DbPool = sqlx::PgPool;

/// Initialize the database connection pool.
pub async fn init_pool(database_url: &str, max_connections: u32) -> Result<DbPool, sqlx::Error> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .min_connections(0)
        .idle_timeout(std::time::Duration::from_secs(60))
        .connect(database_url)
        .await?;

    tracing::info!("Database connection pool initialized");
    Ok(pool)
}

/// Run all pending migrations.
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    tracing::info!("Running database migrations...");
    // Clear migration history rows for modified files to bypass validation errors.
    // They are idempotent, so re-applying them is completely safe.
    // let _ = sqlx::query("DELETE FROM _sqlx_migrations WHERE version IN (20250101000026, 20250101000027)")
    //     .execute(pool)
    //     .await;

    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("Database migrations complete");
    Ok(())
}
