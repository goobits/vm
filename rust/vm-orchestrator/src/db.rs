use crate::error::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::path::Path;
use tracing::instrument;

/// Initialize database connection pool
#[instrument(fields(db_path = %db_path.display()))]
pub async fn create_pool(db_path: &Path) -> Result<SqlitePool> {
    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);

    let pool = SqlitePool::connect_with(options).await?;

    Ok(pool)
}

/// Run database migrations
#[instrument(skip(pool))]
pub async fn run_migrations(pool: &SqlitePool) -> Result<()> {
    sqlx::migrate!("./migrations").run(pool).await?;

    Ok(())
}

/// Backup database before migrations (returns backup path)
pub fn backup_database(db_path: &Path) -> Result<std::path::PathBuf> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let backup_path = db_path.with_extension(format!("db.backup.{}", timestamp));

    if db_path.exists() {
        std::fs::copy(db_path, &backup_path)?;
    }

    Ok(backup_path)
}
