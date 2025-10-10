pub use anyhow::bail;
use std::fmt::{self, Display, Formatter};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VmError {
    Config(String),
    Provider(String),
    Io(#[from] std::io::Error),
    Command(String),
    Dependency(String),
    Network(String),
    Internal(String),
    Filesystem(String),
    Serialization(String),
    Migration(String),
    DockerNotRunning,
    DockerPermission,
    Other(#[from] anyhow::Error),
}

impl Display for VmError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            VmError::Config(s) => write!(f, "Configuration error: {}", s),
            VmError::Provider(s) => write!(f, "Provider error: {}", s),
            VmError::Io(e) => write!(f, "I/O error: {}", e),
            VmError::Command(s) => write!(f, "Command failed: {}", s),
            VmError::Dependency(s) => write!(f, "Dependency not found: {}", s),
            VmError::Network(s) => write!(f, "Network error: {}", s),
            VmError::Internal(s) => write!(f, "Internal error: {}", s),
            VmError::Filesystem(s) => write!(f, "Filesystem error: {}", s),
            VmError::Serialization(s) => write!(f, "Serialization error: {}", s),
            VmError::Migration(s) => write!(f, "Migration error: {}", s),
            VmError::DockerNotRunning => {
                write!(f, "Docker daemon is not running\n\n")?;
                write!(f, "Fix:\n")?;
                write!(f, "  • Start Docker Desktop, or\n")?;
                write!(f, "  • Run: sudo systemctl start docker\n")?;
                write!(f, "  • Verify: docker ps")
            }
            VmError::DockerPermission => {
                write!(f, "Permission denied accessing Docker\n\n")?;
                write!(f, "Fix:\n")?;
                write!(f, "  • Add user to docker group: sudo usermod -aG docker $USER\n")?;
                write!(f, "  • Log out and back in\n")?;
                write!(f, "  • Or use: sudo vm create")
            }
            VmError::Other(e) => write!(f, "Other error: {}", e),
        }
    }
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

pub type Result<T> = std::result::Result<T, VmError>;
