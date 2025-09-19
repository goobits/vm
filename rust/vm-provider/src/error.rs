//! Error types for VM provider operations.
//!
//! This module defines the error types that can occur when working with VM providers
//! such as Docker, Vagrant, and Tart.

use thiserror::Error;
use vm_common::error_messages::{messages, suggestions};

/// Errors that can occur during VM provider operations.
///
/// This enum covers various failure modes when interacting with different VM providers.
#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("Unknown provider: {0}")]
    UnknownProvider(String),

    #[error("Command failed: {0}")]
    CommandFailed(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Dependency not found: {0}")]
    DependencyNotFound(String),
}

impl ProviderError {
    /// Convert provider error to user-friendly message with suggestions
    pub fn user_friendly(&self) -> String {
        match self {
            Self::CommandFailed(cmd)
                if cmd.contains("docker") && cmd.contains("connection refused") =>
            {
                format!(
                    "{}\n💡 {}",
                    messages::DOCKER_NOT_RUNNING,
                    suggestions::START_DOCKER
                )
            }
            Self::CommandFailed(cmd) if cmd.contains("docker: command not found") => {
                format!(
                    "{}\n💡 {}",
                    messages::DOCKER_NOT_INSTALLED,
                    suggestions::INSTALL_DOCKER
                )
            }
            Self::DependencyNotFound(dep) if dep.contains("Docker") => {
                format!(
                    "{}\n💡 {}",
                    messages::DOCKER_NOT_RUNNING,
                    suggestions::START_DOCKER_DESKTOP
                )
            }
            Self::ConfigError(_) => {
                format!(
                    "{}\n💡 {}",
                    messages::CONFIG_NOT_FOUND,
                    suggestions::CREATE_CONFIG
                )
            }
            Self::IoError(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                format!(
                    "{}\n💡 {}",
                    messages::INSUFFICIENT_PERMISSIONS,
                    suggestions::CHECK_PERMISSIONS
                )
            }
            _ => self.to_string(),
        }
    }
}
