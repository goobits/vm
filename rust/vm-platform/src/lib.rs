//! Cross-platform abstraction layer for the VM tool.
//!
//! This crate provides a clean abstraction over platform-specific operations,
//! eliminating the need for scattered `#[cfg]` conditionals throughout the codebase.
//! All platform differences are encapsulated in trait implementations.

pub mod traits;
pub mod providers;
pub mod registry;

// Re-export commonly used items
pub use traits::{PlatformProvider, ShellProvider, ProcessProvider};
pub use registry::PlatformRegistry;

/// Get the current platform provider
pub fn current() -> std::sync::Arc<dyn PlatformProvider> {
    PlatformRegistry::current()
}

/// Convenience functions for common operations
pub mod platform {
    use super::*;
    use anyhow::Result;
    use std::path::PathBuf;

    /// Get the user's configuration directory
    pub fn user_config_dir() -> Result<PathBuf> {
        current().user_config_dir()
    }

    /// Get the user's data directory
    pub fn user_data_dir() -> Result<PathBuf> {
        current().user_data_dir()
    }

    /// Get the user's binary directory
    pub fn user_bin_dir() -> Result<PathBuf> {
        current().user_bin_dir()
    }

    /// Get the user's cache directory
    pub fn user_cache_dir() -> Result<PathBuf> {
        current().user_cache_dir()
    }

    /// Get the user's home directory
    pub fn home_dir() -> Result<PathBuf> {
        current().home_dir()
    }

    /// Get the correct executable name for the platform
    pub fn executable_name(base: &str) -> String {
        current().executable_name(base)
    }

    /// Detect the current shell
    pub fn detect_shell() -> Result<Box<dyn ShellProvider>> {
        current().detect_shell()
    }

    /// Get system CPU core count
    pub fn cpu_core_count() -> Result<u32> {
        current().cpu_core_count()
    }

    /// Get system total memory in GB
    pub fn total_memory_gb() -> Result<u64> {
        current().total_memory_gb()
    }
}