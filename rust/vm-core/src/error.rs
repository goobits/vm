pub use anyhow::bail;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VmError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Command failed: {0}")]
    Command(String),

    #[error("Dependency not found: {0}")]
    Dependency(String),

    #[error("Docker not installed. {0}")]
    DockerNotInstalled(String),

    #[error("Docker daemon is not running. {0}")]
    DockerNotRunning(String),

    #[error("Docker permission denied. {0}")]
    DockerPermission(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Command timeout: {0}")]
    Timeout(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Filesystem error: {0}")]
    Filesystem(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}

impl From<serde_yaml_ng::Error> for VmError {
    fn from(err: serde_yaml_ng::Error) -> Self {
        VmError::Serialization(err.to_string())
    }
}

impl From<serde_json::Error> for VmError {
    fn from(err: serde_json::Error) -> Self {
        VmError::Serialization(err.to_string())
    }
}

impl VmError {
    /// Create a validation error with an optional hint
    pub fn validation<S: Into<String>, H: Into<String>>(msg: S, _hint: Option<H>) -> Self {
        VmError::Validation(msg.into())
    }

    /// Create a filesystem error with path and operation context
    pub fn filesystem<E: std::fmt::Display, P: std::fmt::Display>(
        err: E,
        path: P,
        operation: &str,
    ) -> Self {
        VmError::Filesystem(format!("{} {}: {}", operation, path, err))
    }

    /// Create a general error wrapping another error with context
    pub fn general<E: std::error::Error + Send + Sync + 'static, S: Into<String>>(
        err: E,
        msg: S,
    ) -> Self {
        VmError::Other(anyhow::Error::from(err).context(msg.into()))
    }
}

pub type Result<T> = std::result::Result<T, VmError>;
