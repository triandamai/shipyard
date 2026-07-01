pub mod models;
pub mod redis;
pub mod repo;

/// Re-export the database pool type for convenience.
pub type DbPool = sqlx::PgPool;

/// Initialize the database connection pool.
pub async fn init_pool(database_url: &str, max_connections: u32) -> Result<DbPool, sqlx::Error> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await?;

    tracing::info!("Database connection pool initialized");
    Ok(pool)
}

/// Run all pending migrations.
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::migrate::MigrateError> {
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations").run(pool).await?;
    tracing::info!("Database migrations complete");
    Ok(())
}
