//! Core traits for platform abstraction.
//!
//! This module defines the trait interfaces that abstract away platform-specific
//! operations. Each platform implements these traits to provide consistent
//! behavior across different operating systems.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Core platform abstraction trait.
///
/// This trait encapsulates all platform-specific operations, providing a unified
/// interface for path management, shell operations, binary handling, and system information.
pub trait PlatformProvider: Send + Sync {
    /// Get the platform name (e.g., "unix", "windows", "macos")
    fn name(&self) -> &'static str;

    // === Path Operations ===

    /// Get the user's configuration directory for the VM tool
    ///
    /// # Errors
    ///
    /// Returns an error if the user's configuration directory cannot be determined
    /// or if the platform doesn't support user configuration directories.
    fn user_config_dir(&self) -> Result<PathBuf>;

    /// Get the user's data directory for the VM tool
    ///
    /// # Errors
    ///
    /// Returns an error if the user's data directory cannot be determined
    /// or if the platform doesn't support user data directories.
    fn user_data_dir(&self) -> Result<PathBuf>;

    /// Get the user's binary directory (where executables are installed)
    ///
    /// # Errors
    ///
    /// Returns an error if the user's binary directory cannot be determined
    /// or if the platform doesn't support user binary directories.
    fn user_bin_dir(&self) -> Result<PathBuf>;

    /// Get the user's cache directory for the VM tool
    ///
    /// # Errors
    ///
    /// Returns an error if the user's cache directory cannot be determined
    /// or if the platform doesn't support user cache directories.
    fn user_cache_dir(&self) -> Result<PathBuf>;

    /// Get the user's home directory
    ///
    /// # Errors
    ///
    /// Returns an error if the user's home directory cannot be determined
    /// or if the HOME environment variable is not set.
    fn home_dir(&self) -> Result<PathBuf>;

    /// Get the VM tool's state directory (e.g., ~/.vm)
    ///
    /// # Errors
    ///
    /// Returns an error if the VM state directory cannot be determined
    /// or if the required directories cannot be accessed.
    fn vm_state_dir(&self) -> Result<PathBuf>;

    /// Get the global configuration file path
    ///
    /// # Errors
    ///
    /// Returns an error if the global configuration path cannot be determined
    /// or if the required directories cannot be accessed.
    fn global_config_path(&self) -> Result<PathBuf>;

    /// Get the port registry file path
    ///
    /// # Errors
    ///
    /// Returns an error if the port registry path cannot be determined
    /// or if the required directories cannot be accessed.
    fn port_registry_path(&self) -> Result<PathBuf>;

    // === Shell Operations ===

    /// Detect the current shell and return a shell provider
    ///
    /// # Errors
    ///
    /// Returns an error if the current shell cannot be detected or if no
    /// supported shell provider is available for the detected shell.
    fn detect_shell(&self) -> Result<Box<dyn ShellProvider>>;

    // === Binary Operations ===

    /// Get the correct executable name for the platform (adds .exe on Windows)
    fn executable_name(&self, base: &str) -> String;

    /// Install an executable to the user's bin directory
    /// On Unix: creates symlink, On Windows: copies file and creates wrapper
    ///
    /// # Errors
    ///
    /// Returns an error if the executable cannot be installed, the source file
    /// doesn't exist, the destination directory is not writable, or if platform-specific
    /// installation operations fail (e.g., symlink creation on Unix, file copying on Windows).
    fn install_executable(&self, source: &Path, dest_dir: &Path, name: &str) -> Result<()>;

    // === Package Manager Paths ===

    /// Get the Cargo home directory (where Rust packages are installed)
    ///
    /// # Errors
    ///
    /// Returns an error if the Cargo home directory cannot be determined
    /// or if the required environment variables are not set.
    fn cargo_home(&self) -> Result<PathBuf>;

    /// Get the Cargo binary directory
    ///
    /// # Errors
    ///
    /// Returns an error if the Cargo binary directory cannot be determined
    /// or if the Cargo home directory is not accessible.
    fn cargo_bin_dir(&self) -> Result<PathBuf>;

    /// Get the NPM global directory (if available)
    ///
    /// # Errors
    ///
    /// Returns an error if there's a failure detecting NPM installation
    /// or if NPM commands fail during directory detection.
    fn npm_global_dir(&self) -> Result<Option<PathBuf>>;

    /// Get the NVM versions directory (Node Version Manager)
    ///
    /// # Errors
    ///
    /// Returns an error if there's a failure detecting NVM installation
    /// or if the NVM directory structure cannot be accessed.
    fn nvm_versions_dir(&self) -> Result<Option<PathBuf>>;

    /// Get Python site-packages directories
    ///
    /// # Errors
    ///
    /// Returns an error if Python installation cannot be detected
    /// or if site-packages directories cannot be determined.
    fn python_site_packages(&self) -> Result<Vec<PathBuf>>;

    // === System Information ===

    /// Get the number of CPU cores
    ///
    /// # Errors
    ///
    /// Returns an error if the CPU core count cannot be determined
    /// or if system information is not accessible.
    fn cpu_core_count(&self) -> Result<u32>;

    /// Get total system memory in GB
    ///
    /// # Errors
    ///
    /// Returns an error if the system memory information cannot be determined
    /// or if system information is not accessible.
    fn total_memory_gb(&self) -> Result<u64>;

    // === Process Operations ===

    /// Get the PATH environment variable separator
    fn path_separator(&self) -> char;

    /// Split PATH environment variable into individual paths
    fn split_path_env(&self, path: &str) -> Vec<PathBuf>;

    /// Join paths into a PATH environment variable string
    fn join_path_env(&self, paths: &[PathBuf]) -> String;
}

/// Shell abstraction trait.
///
/// This trait provides a unified interface for different shell types,
/// handling profile paths and command syntax differences.
pub trait ShellProvider: Send + Sync {
    /// Get the shell name (e.g., "bash", "zsh", "powershell")
    fn name(&self) -> &'static str;

    /// Get the path to the shell's profile/config file
    fn profile_path(&self) -> Option<PathBuf>;

    /// Generate the syntax to add a directory to PATH
    fn path_export_syntax(&self, path: &Path) -> String;

    /// Create the profile file if it doesn't exist
    ///
    /// # Errors
    ///
    /// Returns an error if the profile file cannot be created, the parent
    /// directory is not writable, or if file system operations fail.
    fn create_profile_if_missing(&self) -> Result<()>;

    /// Check if this shell is currently active
    fn is_active(&self) -> bool;
}

/// Process execution abstraction trait.
///
/// This trait handles platform-specific process execution details.
pub trait ProcessProvider: Send + Sync {
    /// Prepare a command for execution (set platform-specific environment, etc.)
    ///
    /// # Errors
    ///
    /// Returns an error if platform-specific command preparation fails
    /// or if required environment variables cannot be set.
    fn prepare_command(&self, cmd: &mut Command) -> Result<()>;

    /// Get the default shell command for the platform
    fn default_shell_command(&self) -> (&'static str, Vec<&'static str>);

    /// Check if a command/executable exists in PATH
    fn command_exists(&self, command: &str) -> bool;
}
