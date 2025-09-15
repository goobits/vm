//! Error types for VM provider operations.
//!
//! This module defines the error types that can occur when working with VM providers
//! such as Docker, Vagrant, and Tart.

use thiserror::Error;

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
