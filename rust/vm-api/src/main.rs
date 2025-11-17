use anyhow::Result;
use std::path::PathBuf;
use tracing::info;
use vm_api::{create_app, start_janitor_task, start_provisioner_task};
use vm_orchestrator::db::{backup_database, create_pool, run_migrations};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("vm_api=debug,vm_orchestrator=debug,tower_http=debug")
        .init();

    info!("Starting vm-api service...");

    // Database setup
    let db_path = get_db_path();
    info!("Database path: {}", db_path.display());

    // Backup before migrations
    if db_path.exists() {
        let backup_path = backup_database(&db_path)?;
        info!("Database backed up to: {}", backup_path.display());
    }

    // Create pool and run migrations
    let pool = create_pool(&db_path).await?;
    info!("Running database migrations...");
    run_migrations(&pool).await?;
    info!("Migrations complete");

    // Start janitor task
    tokio::spawn(start_janitor_task(pool.clone()));
    info!("Janitor task started");

    // Start provisioner task
    tokio::spawn(start_provisioner_task(pool.clone()));
    info!("Provisioner task started");

    // Create app
    let app = create_app(pool).await?;

    // Start server
    let addr = "0.0.0.0:3000";
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Listening on http://{}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

fn get_db_path() -> PathBuf {
    if cfg!(windows) {
        let appdata = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(appdata).join("vm").join("api").join("vm.db")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".vm").join("api").join("vm.db")
    }
}
