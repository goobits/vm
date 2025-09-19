//! Cross-platform user directory utilities for the VM tool.
//!
//! This module provides platform-agnostic functions for accessing user-specific
//! directories like configuration, data, and binary directories. It delegates
//! to the vm-platform crate for core functionality while providing backward
//! compatibility and higher-level convenience functions.

use anyhow::Result;
use std::path::PathBuf;

/// Get the user's configuration directory for the VM tool.
///
/// Returns:
/// - Linux/macOS: `~/.config/vm` or `$XDG_CONFIG_HOME/vm`
/// - Windows: `%APPDATA%\vm`
#[must_use = "configuration directory path should be used"]
pub fn user_config_dir() -> Result<PathBuf> {
    vm_platform::platform::user_config_dir()
}

/// Get the user's data directory for the VM tool.
///
/// Returns:
/// - Linux: `~/.local/share/vm` or `$XDG_DATA_HOME/vm`
/// - macOS: `~/Library/Application Support/vm`
/// - Windows: `%LOCALAPPDATA%\vm`
#[must_use = "data directory path should be used"]
pub fn user_data_dir() -> Result<PathBuf> {
    vm_platform::platform::user_data_dir()
}

/// Get the user's binary directory for installing executables.
///
/// Returns:
/// - Linux/macOS: `~/.local/bin`
/// - Windows: `%LOCALAPPDATA%\vm\bin`
#[must_use = "binary directory path should be used"]
pub fn user_bin_dir() -> Result<PathBuf> {
    vm_platform::platform::user_bin_dir()
}

/// Get the VM tool's state directory.
///
/// Returns:
/// - Linux/macOS: `~/.vm`
/// - Windows: `%USERPROFILE%\.vm`
#[must_use = "state directory path should be used"]
pub fn vm_state_dir() -> Result<PathBuf> {
    vm_platform::platform::vm_state_dir()
}

/// Get the user's cache directory for the VM tool.
///
/// Returns:
/// - Linux: `~/.cache/vm` or `$XDG_CACHE_HOME/vm`
/// - macOS: `~/Library/Caches/vm`
/// - Windows: `%LOCALAPPDATA%\vm\cache`
#[must_use = "cache directory path should be used"]
pub fn user_cache_dir() -> Result<PathBuf> {
    vm_platform::platform::user_cache_dir()
}

/// Get the global configuration path for the VM tool.
///
/// Returns the path to the global configuration file:
/// - Linux/macOS: `~/.config/vm/global.yaml`
/// - Windows: `%APPDATA%\vm\global.yaml`
#[must_use = "global configuration path should be used"]
pub fn global_config_path() -> Result<PathBuf> {
    Ok(user_config_dir()?.join("global.yaml"))
}

/// Get the port registry path for the VM tool.
///
/// Returns the path to the port registry file:
/// - All platforms: `~/.vm/port-registry.json`
#[must_use = "port registry path should be used"]
pub fn port_registry_path() -> Result<PathBuf> {
    Ok(vm_state_dir()?.join("port-registry.json"))
}

/// Get the user's home directory.
///
/// This is a convenience wrapper that returns a Result with a proper error message.
#[must_use = "home directory path should be used"]
pub fn home_dir() -> Result<PathBuf> {
    vm_platform::platform::home_dir()
}

/// Get the user's documents directory.
///
/// Returns:
/// - Linux/macOS: `~/Documents`
/// - Windows: `%USERPROFILE%\Documents`
#[must_use = "documents directory path should be used"]
pub fn documents_dir() -> Result<PathBuf> {
    use anyhow::Context;
    dirs::document_dir().context("Could not determine documents directory")
}

/// Check if a path looks like a Windows path (contains backslashes or drive letters).
pub fn is_windows_path(path: &str) -> bool {
    path.contains('\\') || (path.len() >= 2 && path.chars().nth(1) == Some(':'))
}

/// Convert a Unix-style path to the appropriate platform format.
///
/// On Unix systems, this is a no-op. On Windows, it converts forward slashes
/// to backslashes and handles drive letters.
pub fn to_platform_path(path: &str) -> String {
    #[cfg(windows)]
    {
        path.replace('/', "\\")
    }

    #[cfg(not(windows))]
    {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_config_dir() {
        let result = user_config_dir();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("vm"));
    }

    #[test]
    fn test_user_data_dir() {
        let result = user_data_dir();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with("vm"));
    }

    #[test]
    fn test_vm_state_dir() {
        let result = vm_state_dir();
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with(".vm"));
    }

    #[test]
    fn test_is_windows_path() {
        assert!(is_windows_path("C:\\Users\\test"));
        assert!(is_windows_path("D:\\Program Files\\app"));
        assert!(is_windows_path("path\\to\\file"));
        assert!(!is_windows_path("/usr/bin"));
        assert!(!is_windows_path("./relative/path"));
    }

    #[test]
    fn test_to_platform_path() {
        #[cfg(windows)]
        {
            assert_eq!(to_platform_path("/usr/bin"), "\\usr\\bin");
            assert_eq!(to_platform_path("path/to/file"), "path\\to\\file");
        }

        #[cfg(not(windows))]
        {
            assert_eq!(to_platform_path("/usr/bin"), "/usr/bin");
            assert_eq!(to_platform_path("path/to/file"), "path/to/file");
        }
    }

    #[test]
    fn test_all_paths_exist_or_creatable() {
        // Test that all path functions return valid results
        assert!(user_config_dir().is_ok());
        assert!(user_data_dir().is_ok());
        assert!(user_bin_dir().is_ok());
        assert!(vm_state_dir().is_ok());
        assert!(user_cache_dir().is_ok());
        assert!(global_config_path().is_ok());
        assert!(port_registry_path().is_ok());
        assert!(home_dir().is_ok());

        // documents_dir() might not be available in test environment
        match documents_dir() {
            Ok(_) => {}
            Err(e) => println!("documents_dir() failed: {}", e),
        }
    }
}
