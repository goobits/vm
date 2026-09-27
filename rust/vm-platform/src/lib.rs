//! Cross-platform abstraction layer for the VM tool.
//!
//! This crate provides a clean abstraction over platform-specific operations,
//! eliminating the need for scattered `#[cfg]` conditionals throughout the codebase.
//! All platform differences are encapsulated in trait implementations.

mod host;
pub mod providers;
pub mod registry;
pub mod traits;

// Re-export commonly used items
pub use registry::PlatformRegistry;
pub use traits::{PlatformProvider, ProcessProvider, ShellProvider};

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

    /// Get the user's documents directory
    pub fn documents_dir() -> Result<PathBuf> {
        current().documents_dir()
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

    /// Get system total memory in bytes
    pub fn total_memory_bytes() -> Result<u64> {
        current().total_memory_bytes()
    }

    /// Get system total memory in megabytes
    pub fn total_memory_mb() -> Result<u64> {
        Ok(total_memory_bytes()? / 1024 / 1024)
    }

    /// Get system total memory in gigabytes
    pub fn total_memory_gb() -> Result<u64> {
        Ok(total_memory_bytes()? / 1024 / 1024 / 1024)
    }

    /// Get the logical parallelism available to this process
    pub fn available_parallelism() -> usize {
        std::thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1)
    }

    pub fn operating_system() -> &'static str {
        crate::host::operating_system()
    }

    pub fn architecture() -> &'static str {
        crate::host::architecture()
    }

    pub fn detect_host_os() -> String {
        crate::host::detect_host_os()
    }

    pub fn detect_timezone() -> String {
        crate::host::detect_timezone()
    }

    pub fn current_uid() -> u32 {
        crate::host::current_uid()
    }

    pub fn current_gid() -> u32 {
        crate::host::current_gid()
    }

    /// Get Docker host gateway address for container-to-host communication
    pub fn get_host_gateway() -> &'static str {
        if cfg!(target_os = "linux") {
            "172.17.0.1"
        } else {
            "host.docker.internal"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn state_directory_follows_the_platform_home() {
        let home = platform::home_dir().expect("should get home dir");
        assert_eq!(
            home,
            dirs::home_dir().expect("native home must be available")
        );
        assert_eq!(platform::vm_state_dir().unwrap(), home.join(".vm"));
    }

    #[test]
    fn test_platform_respects_home_env() {
        const CHILD_HOME: &str = "VM_PLATFORM_TEST_HOME";
        if let Some(expected) = std::env::var_os(CHILD_HOME) {
            let expected = std::path::PathBuf::from(expected);
            assert_eq!(platform::home_dir().unwrap(), expected);
            assert_eq!(platform::vm_state_dir().unwrap(), expected.join(".vm"));
            return;
        }

        // Isolate the environment override from concurrent native path lookups.
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::test_platform_respects_home_env",
                "--nocapture",
            ])
            .env(
                if cfg!(windows) { "USERPROFILE" } else { "HOME" },
                directory.path(),
            )
            .env(CHILD_HOME, directory.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_profile_fallback_and_invalid_override() {
        const CHILD_MODE: &str = "VM_PLATFORM_TEST_PROFILE_MODE";
        if let Ok(mode) = std::env::var(CHILD_MODE) {
            if mode == "relative" {
                assert!(platform::home_dir()
                    .unwrap_err()
                    .to_string()
                    .contains("absolute"));
            } else {
                assert_eq!(platform::home_dir().unwrap(), dirs::home_dir().unwrap());
            }
            return;
        }
        for mode in ["absent", "empty", "relative"] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "tests::windows_profile_fallback_and_invalid_override",
                    "--nocapture",
                ])
                .env(CHILD_MODE, mode);
            match mode {
                "absent" => {
                    command.env_remove("USERPROFILE");
                }
                "empty" => {
                    command.env("USERPROFILE", "");
                }
                _ => {
                    command.env("USERPROFILE", "relative/profile");
                }
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn resource_detection_returns_host_capacity() {
        assert!(platform::cpu_core_count().expect("should detect CPU cores") > 0);
        assert!(platform::total_memory_bytes().expect("should detect memory") > 0);
        assert!(platform::available_parallelism() > 0);
    }
}
