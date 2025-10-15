pub use anyhow::bail;
use thiserror::Error;

/// The primary error type for the `vm-core` crate.
///
/// This enum consolidates all possible errors that can occur within the `vm-core` crate,
/// providing a unified error handling mechanism. Each variant corresponds to a specific
/// category of error, making it easier to handle different failure modes.
#[derive(Error, Debug)]
pub enum VmError {
    /// An error related to configuration, such as a missing or invalid `vm.yaml` file.
    #[error("Configuration error: {0}")]
    Config(String),

    /// An error originating from a VM provider, such as Docker or Vagrant.
    #[error("Provider error: {0}")]
    Provider(String),

    /// An I/O error, typically related to file system operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// An error that occurs when a command fails to execute.
    #[error("Command failed: {0}")]
    Command(String),

    /// An error indicating that a required dependency is not found on the system.
    #[error("Dependency not found: {0}")]
    Dependency(String),

    /// An error indicating that Docker is not installed on the system.
    #[error("Docker not installed. {0}")]
    DockerNotInstalled(String),

    /// An error indicating that the Docker daemon is not running.
    #[error("Docker daemon is not running. {0}")]
    DockerNotRunning(String),

    /// An error indicating that the current user does not have permission to access the Docker daemon.
    #[error("Docker permission denied. {0}")]
    DockerPermission(String),

    /// A network-related error, such as a failure to connect to a service.
    #[error("Network error: {0}")]
    Network(String),

    /// An unexpected internal error.
    #[error("Internal error: {0}")]
    Internal(String),

    /// An error indicating that a requested resource was not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// A file system-related error.
    #[error("Filesystem error: {0}")]
    Filesystem(String),

    /// An error related to serialization or deserialization of data.
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// An error that occurs during a data migration.
    #[error("Migration error: {0}")]
    Migration(String),

    /// A catch-all for any other type of error, typically from `anyhow`.
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

/// A specialized `Result` type for the `vm-core` crate.
///
/// This type alias is used throughout the `vm-core` crate for functions that can return
/// a `VmError`. It simplifies the function signatures and makes the code more readable.
pub type Result<T> = std::result::Result<T, VmError>;
