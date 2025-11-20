use sqlx::SqlitePool;

/// Helper to create an in-memory test database with migrations applied
pub async fn create_test_db() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    // Run migrations
    // The path is relative to the source file.
    // src/test_utils.rs -> ../migrations points to vm-orchestrator/migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}
