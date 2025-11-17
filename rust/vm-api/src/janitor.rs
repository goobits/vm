use sqlx::SqlitePool;
use tokio::time::{interval, Duration};
use tracing::{error, info};
use vm_orchestrator::WorkspaceOrchestrator;

pub async fn start_janitor_task(pool: SqlitePool, interval_secs: u64) {
    let orchestrator = WorkspaceOrchestrator::new(pool);
    let mut interval = interval(Duration::from_secs(interval_secs));

    info!(
        "Janitor task running (checks every {} seconds)",
        interval_secs
    );

    loop {
        interval.tick().await;

        if let Err(e) = cleanup_expired_workspaces(&orchestrator).await {
            error!("Janitor cleanup failed: {}", e);
        }
    }
}

async fn cleanup_expired_workspaces(orchestrator: &WorkspaceOrchestrator) -> anyhow::Result<()> {
    let expired = orchestrator.get_expired_workspaces().await?;

    for workspace in expired {
        info!(
            "TTL expired, deleting workspace: {} ({})",
            workspace.name, workspace.id
        );

        if let Err(e) = orchestrator.delete_workspace(&workspace.id).await {
            error!("Failed to delete expired workspace {}: {}", workspace.id, e);
        }
    }

    Ok(())
}
