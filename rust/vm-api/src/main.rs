use anyhow::Result;
use tracing::info;
use vm_api::{create_app, start_janitor_task, start_provisioner_task, Config};
use vm_orchestrator::db::{backup_database, create_pool, run_migrations};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("vm_api=debug,vm_orchestrator=debug,tower_http=debug")
        .init();

    info!("Starting vm-api service...");

    // Load configuration
    let config = Config::from_env();
    info!(
        "Configuration loaded: bind_addr={}, db_path={}",
        config.bind_addr,
        config.db_path.display()
    );

    // Database setup
    let db_path = &config.db_path;
    info!("Database path: {}", db_path.display());

    // Backup before migrations
    if db_path.exists() {
        let backup_path = backup_database(db_path)?;
        info!("Database backed up to: {}", backup_path.display());
    }

    // Create pool and run migrations
    let pool = create_pool(db_path).await?;
    info!("Running database migrations...");
    run_migrations(&pool).await?;
    info!("Migrations complete");

    // Start janitor task
    tokio::spawn(start_janitor_task(
        pool.clone(),
        config.janitor_interval_secs,
    ));
    info!(
        "Janitor task started (interval: {}s)",
        config.janitor_interval_secs
    );

    // Start provisioner task
    tokio::spawn(start_provisioner_task(
        pool.clone(),
        config.provisioner_interval_secs,
    ));
    info!(
        "Provisioner task started (interval: {}s)",
        config.provisioner_interval_secs
    );

    // Create app
    let app = create_app(pool).await?;

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.bind_addr).await?;
    info!("Listening on http://{}", config.bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
