use sqlx::SqlitePool;
use vm_orchestrator::WorkspaceOrchestrator;

#[derive(Clone)]
pub struct AppState {
    pub orchestrator: WorkspaceOrchestrator,
}

impl AppState {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            orchestrator: WorkspaceOrchestrator::new(pool),
        }
    }
}
