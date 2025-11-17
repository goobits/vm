use thiserror::Error;

pub type Result<T> = std::result::Result<T, OrchestratorError>;

#[derive(Error, Debug)]
pub enum OrchestratorError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Workspace not found: {0}")]
    NotFound(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Provider error: {0}")]
    Provider(#[from] anyhow::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
