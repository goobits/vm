//! Temporary VM state management.
//!
//! This module provides functionality for persisting and managing the state of temporary VMs,
//! including state file operations, locking mechanisms, and validation.

use crate::TempVmState;
use fs2::FileExt;
use serde_yaml_ng as serde_yaml;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;
use vm_core::error::{Result, VmError};

/// Errors that can occur during state management operations.
#[derive(Error, Debug)]
pub enum StateError {
    #[error("State file not found at {path}")]
    StateNotFound { path: PathBuf },
    #[error("Invalid state file format: {0}")]
    InvalidFormat(#[from] serde_yaml::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("VM error: {0}")]
    Vm(#[from] VmError),
    #[error("State validation failed: {reason}")]
    ValidationFailed { reason: String },
}

impl From<StateError> for VmError {
    fn from(err: StateError) -> Self {
        match err {
            StateError::StateNotFound { path } => {
                VmError::Config(format!("State file not found at {}", path.display()))
            }
            StateError::InvalidFormat(e) => {
                VmError::Serialization(format!("Invalid state file format: {e}"))
            }
            StateError::Io(e) => VmError::Io(e),
            StateError::Vm(e) => e,
            StateError::ValidationFailed { reason } => {
                VmError::Config(format!("State validation failed: {reason}"))
            }
        }
    }
}

/// Manages temp VM state persistence and validation
#[derive(Debug)]
pub struct StateManager {
    state_dir: PathBuf,
    state_file: PathBuf,
    temp_file_registry: PathBuf,
    lock_file: PathBuf,
}

impl StateManager {
    /// Creates a new state manager with the default state directory.
    ///
    /// The default state directory is `~/.vm`.
    ///
    /// # Returns
    /// A `Result` containing the new `StateManager` or an error if initialization fails.
    pub fn new() -> Result<Self> {
        let state_dir = Self::default_state_dir()?;
        fs::create_dir_all(&state_dir)?;
        let state_file = vm_core::user_paths::temp_vms_state_path()?;
        let temp_file_registry = state_dir.join(".temp_files.registry");
        let lock_file = state_dir.join(".temp-vm.lock");

        Ok(Self {
            state_dir,
            state_file,
            temp_file_registry,
            lock_file,
        })
    }

    /// Creates a new state manager with a custom state directory.
    ///
    /// # Arguments
    /// * `state_dir` - The custom directory to use for state files
    ///
    /// # Returns
    /// A new `StateManager` instance using the specified directory.
    pub fn with_state_dir(state_dir: PathBuf) -> Self {
        let state_file = state_dir.join("temp-vm.state");
        let temp_file_registry = state_dir.join(".temp_files.registry");
        let lock_file = state_dir.join(".temp-vm.lock");
        Self {
            state_dir,
            state_file,
            temp_file_registry,
            lock_file,
        }
    }

    /// Gets the default state directory path.
    ///
    /// Returns `~/.vm` where `~` is the user's home directory.
    ///
    /// # Returns
    /// A `Result` containing the default state directory path or an error if the home directory cannot be found.
    pub fn default_state_dir() -> Result<PathBuf> {
        vm_platform::platform::vm_state_dir()
            .map_err(|e| VmError::Internal(format!("Failed to get VM state directory: {e}")))
    }

    /// Get the state file path
    pub fn state_file_path(&self) -> &Path {
        &self.state_file
    }

    /// Check if a temp VM state exists
    pub fn state_exists(&self) -> bool {
        self.state_file.exists()
    }

    /// Acquire an exclusive lock for state operations
    fn acquire_lock(&self) -> std::result::Result<File, StateError> {
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.lock_file)?;

        lock_file.lock_exclusive().map_err(|e| {
            StateError::Vm(VmError::Internal(format!(
                "Failed to acquire lock {}: {}",
                self.lock_file.display(),
                e
            )))
        })?;

        Ok(lock_file)
    }

    /// Load temp VM state from disk
    pub fn load_state(&self) -> std::result::Result<TempVmState, StateError> {
        let _lock = self.acquire_lock()?;

        if !self.state_file.exists() {
            return Err(StateError::StateNotFound {
                path: self.state_file.clone(),
            });
        }

        let content =
            fs::read_to_string(&self.state_file).map_err(|e| StateError::Vm(VmError::Io(e)))?;

        let state: TempVmState = serde_yaml::from_str(&content).map_err(|e| {
            StateError::Vm(VmError::Serialization(format!(
                "Failed to parse state file {}: {}",
                self.state_file.display(),
                e
            )))
        })?;

        Self::validate_state(&state)?;
        Ok(state)
    }

    /// Save temp VM state to disk atomically
    pub fn save_state(&self, state: &TempVmState) -> std::result::Result<(), StateError> {
        let _lock = self.acquire_lock()?;

        Self::validate_state(state)?;

        // Create state directory if it doesn't exist
        fs::create_dir_all(&self.state_dir).map_err(|e| {
            StateError::Vm(VmError::Filesystem(format!(
                "Failed to create state directory {}: {}",
                self.state_dir.display(),
                e
            )))
        })?;

        // Serialize state to YAML
        let yaml_content = serde_yaml::to_string(state).map_err(|e| {
            StateError::Vm(VmError::Serialization(format!(
                "Failed to serialize state to YAML: {e}"
            )))
        })?;

        // Write atomically using a unique temporary file
        let temp_file = tempfile::Builder::new()
            .prefix("temp-vm-state-")
            .suffix(".tmp")
            .tempfile_in(&self.state_dir)?;

        temp_file
            .as_file()
            .write_all(yaml_content.as_bytes())
            .map_err(|e| {
                StateError::Vm(VmError::Filesystem(format!(
                    "Failed to write temporary state file {}: {}",
                    temp_file.path().display(),
                    e
                )))
            })?;

        // Atomic move to final location
        temp_file.persist(&self.state_file).map_err(|e| {
            StateError::Vm(VmError::Filesystem(format!(
                "Failed to move state file to final location {}: {}",
                self.state_file.display(),
                e.error
            )))
        })?;

        Ok(())
    }

    /// Delete the state file
    pub fn delete_state(&self) -> std::result::Result<(), StateError> {
        let _lock = self.acquire_lock()?;

        if self.state_file.exists() {
            fs::remove_file(&self.state_file).map_err(|e| {
                StateError::Vm(VmError::Filesystem(format!(
                    "Failed to delete state file {}: {}",
                    self.state_file.display(),
                    e
                )))
            })?;
        }
        Ok(())
    }

    /// Creates a new temporary file and registers it for cleanup.
    ///
    /// # Arguments
    /// * `prefix` - The prefix to use for the temporary file name
    ///
    /// # Returns
    /// A `Result` containing the path to the created temporary file.
    pub fn create_temp_file(&self, prefix: &str) -> Result<PathBuf> {
        let temp_file = tempfile::Builder::new()
            .prefix(prefix)
            .tempfile_in(&self.state_dir)?;
        let path = temp_file.into_temp_path().to_path_buf();

        let mut registry = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.temp_file_registry)?;
        writeln!(registry, "{}", path.display())?;

        Ok(path)
    }

    /// Cleans up all registered temporary files.
    ///
    /// This method removes all files that were registered through `create_temp_file`.
    /// It silently ignores any errors that occur during cleanup.
    pub fn cleanup_temp_files(&self) {
        if !self.temp_file_registry.exists() {
            return;
        }

        if let Ok(content) = fs::read_to_string(&self.temp_file_registry) {
            for path_str in content.lines() {
                let path = Path::new(path_str);
                if path.is_file() {
                    let _ = fs::remove_file(path);
                } else if path.is_dir() {
                    let _ = fs::remove_dir_all(path);
                }
            }
            let _ = fs::remove_file(&self.temp_file_registry);
        }
    }

    /// Validate temp VM state for consistency and security
    pub fn validate_state(state: &TempVmState) -> std::result::Result<(), StateError> {
        // Validate container name is not empty
        if state.container_name.trim().is_empty() {
            return Err(StateError::ValidationFailed {
                reason: "Container name cannot be empty".into(),
            });
        }

        // Validate provider is not empty
        if state.provider.trim().is_empty() {
            return Err(StateError::ValidationFailed {
                reason: "Provider cannot be empty".into(),
            });
        }

        // Validate project directory exists
        if !state.project_dir.exists() {
            return Err(StateError::ValidationFailed {
                reason: format!(
                    "Project directory does not exist: {}",
                    state.project_dir.display()
                ),
            });
        }

        // Validate all mount sources exist and are directories
        for mount in &state.mounts {
            if !mount.source.exists() {
                return Err(StateError::ValidationFailed {
                    reason: format!("Mount source does not exist: {}", mount.source.display()),
                });
            }

            if !mount.source.is_dir() {
                return Err(StateError::ValidationFailed {
                    reason: format!(
                        "Mount source is not a directory: {}",
                        mount.source.display()
                    ),
                });
            }

            // Security check: prevent mounting dangerous system directories
            if Self::is_dangerous_mount_source(&mount.source) {
                return Err(StateError::ValidationFailed {
                    reason: format!(
                        "Dangerous mount source not allowed: {}",
                        mount.source.display()
                    ),
                });
            }
        }

        Ok(())
    }

    /// Get platform-specific temp directory paths
    fn get_platform_temp_paths() -> Vec<&'static str> {
        #[cfg(target_os = "windows")]
        {
            vec![] // Windows temp is handled by std::env::temp_dir()
        }
        #[cfg(target_os = "macos")]
        {
            vec![
                "/tmp",
                "/var/tmp",
                "/var/folders",
                "/private/tmp",
                "/private/var/tmp",
            ]
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            // Linux and other Unix-like systems
            vec!["/tmp", "/var/tmp", "/dev/shm"]
        }
    }

    /// Check if a path is dangerous to mount (system directories)
    fn is_dangerous_mount_source(path: &Path) -> bool {
        // Allow paths inside system temp directories
        if let Ok(temp_dir) = std::env::temp_dir().canonicalize() {
            if let Ok(canonical_path) = path.canonicalize() {
                if canonical_path.starts_with(&temp_dir) {
                    return false;
                }
            }
        }

        // Platform-specific additional temp directories
        let additional_temp_paths = Self::get_platform_temp_paths();
        for temp_path in &additional_temp_paths {
            if path.starts_with(temp_path) {
                return false;
            }
        }

        let dangerous_paths = [
            "/", "/etc", "/usr", "/var", "/bin", "/sbin", "/boot", "/sys", "/proc", "/dev", "/root",
        ];

        // Check exact matches and if path starts with dangerous paths
        for dangerous in &dangerous_paths {
            let dangerous_path = Path::new(dangerous);
            if path == dangerous_path {
                return true;
            }
            // For non-root paths, check if it's a subdirectory of dangerous paths
            if *dangerous != "/" && path.starts_with(dangerous_path) {
                return true;
            }
        }

        false
    }
}

// Safe default implementation with fallback to current directory
impl Default for StateManager {
    fn default() -> Self {
        match Self::new() {
            Ok(manager) => manager,
            Err(_) => {
                // Fallback to current directory if state directory creation fails
                let fallback_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                Self {
                    state_dir: fallback_dir.clone(),
                    state_file: fallback_dir.join(".vm_temp_state.yaml"),
                    temp_file_registry: fallback_dir.join(".temp_files.registry"),
                    lock_file: fallback_dir.join(".temp_vm.lock"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dangerous_mount_detection() {
        // These should be dangerous
        assert!(StateManager::is_dangerous_mount_source(Path::new("/")));
        assert!(StateManager::is_dangerous_mount_source(Path::new("/etc")));
        assert!(StateManager::is_dangerous_mount_source(Path::new(
            "/usr/bin"
        )));
        assert!(StateManager::is_dangerous_mount_source(Path::new(
            "/var/log"
        )));
        assert!(StateManager::is_dangerous_mount_source(Path::new("/root")));

        // These should be safe
        assert!(!StateManager::is_dangerous_mount_source(Path::new(
            "/home/user"
        )));
        assert!(!StateManager::is_dangerous_mount_source(Path::new("/tmp")));
        assert!(!StateManager::is_dangerous_mount_source(Path::new(
            "/tmp/test"
        )));
        assert!(!StateManager::is_dangerous_mount_source(Path::new(
            "/var/tmp"
        )));

        // Platform-specific safe paths
        #[cfg(target_os = "macos")]
        assert!(!StateManager::is_dangerous_mount_source(Path::new(
            "/var/folders/test"
        )));

        #[cfg(not(target_os = "macos"))]
        assert!(StateManager::is_dangerous_mount_source(Path::new(
            "/var/folders/test"
        )));

        assert!(!StateManager::is_dangerous_mount_source(Path::new(
            "/workspace"
        )));
    }
}
