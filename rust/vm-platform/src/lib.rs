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

    /// Get the VM tool's state directory
    pub fn vm_state_dir() -> Result<PathBuf> {
        current().vm_state_dir()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_platform_respects_home_env() {
        let original_home = env::var("HOME").ok();

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let test_home = temp_dir.path().to_path_buf();

        // Set HOME to our test directory
        env::set_var("HOME", &test_home);

        // Test that vm_state_dir() uses the test HOME
        let state_dir = platform::vm_state_dir().unwrap();
        assert!(state_dir.starts_with(&test_home));
        assert!(state_dir.ends_with(".vm"));

        // Test that home_dir() returns the test HOME
        let home = platform::home_dir().unwrap();
        assert_eq!(home, test_home);

        // Restore original HOME
        match original_home {
            Some(original) => env::set_var("HOME", original),
            None => env::remove_var("HOME"),
        }
    }
}