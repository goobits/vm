//! Temporary VM state management.
//!
//! This module provides functionality for persisting and managing the state of temporary VMs,
//! including state file operations, locking mechanisms, and validation.

use crate::TempVmState;
use anyhow::{Context, Result};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during state management operations.
#[derive(Error, Debug)]
pub enum StateError {
    #[error("State file not found at {path}")]
    StateNotFound { path: PathBuf },
    #[error("Invalid state file format: {0}")]
    InvalidFormat(#[from] serde_yaml::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Context error: {0}")]
    Context(#[from] anyhow::Error),
    #[error("State validation failed: {reason}")]
    ValidationFailed { reason: String },
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
        let state_file = state_dir.join("temp-vm.state");
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
        dirs::home_dir()
            .map(|home| home.join(".vm"))
            .ok_or_else(|| anyhow::anyhow!("Could not find home directory"))
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
    fn acquire_lock(&self) -> Result<File, StateError> {
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&self.lock_file)?;

        lock_file
            .lock_exclusive()
            .with_context(|| format!("Failed to acquire lock: {}", self.lock_file.display()))?;

        Ok(lock_file)
    }

    /// Load temp VM state from disk
    pub fn load_state(&self) -> Result<TempVmState, StateError> {
        let _lock = self.acquire_lock()?;

        if !self.state_file.exists() {
            return Err(StateError::StateNotFound {
                path: self.state_file.clone(),
            });
        }

        let content = fs::read_to_string(&self.state_file)
            .with_context(|| format!("Failed to read state file: {}", self.state_file.display()))?;

        let state: TempVmState = serde_yaml::from_str(&content).with_context(|| {
            format!("Failed to parse state file: {}", self.state_file.display())
        })?;

        self.validate_state(&state)?;
        Ok(state)
    }

    /// Save temp VM state to disk atomically
    pub fn save_state(&self, state: &TempVmState) -> Result<(), StateError> {
        let _lock = self.acquire_lock()?;

        self.validate_state(state)?;

        // Create state directory if it doesn't exist
        fs::create_dir_all(&self.state_dir).with_context(|| {
            format!(
                "Failed to create state directory: {}",
                self.state_dir.display()
            )
        })?;

        // Serialize state to YAML
        let yaml_content =
            serde_yaml::to_string(state).with_context(|| "Failed to serialize state to YAML")?;

        // Write atomically using a unique temporary file
        let temp_file = tempfile::Builder::new()
            .prefix("temp-vm-state-")
            .suffix(".tmp")
            .tempfile_in(&self.state_dir)?;

        temp_file
            .as_file()
            .write_all(yaml_content.as_bytes())
            .with_context(|| {
                format!(
                    "Failed to write temporary state file: {}",
                    temp_file.path().display()
                )
            })?;

        // Atomic move to final location
        temp_file
            .persist(&self.state_file)
            .map_err(|e| e.error)
            .with_context(|| {
                format!(
                    "Failed to move state file to final location: {}",
                    self.state_file.display()
                )
            })?;

        Ok(())
    }

    /// Delete the state file
    pub fn delete_state(&self) -> Result<(), StateError> {
        let _lock = self.acquire_lock()?;

        if self.state_file.exists() {
            fs::remove_file(&self.state_file).with_context(|| {
                format!("Failed to delete state file: {}", self.state_file.display())
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

        let mut registry = fs::OpenOptions::new()
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
    pub fn validate_state(&self, state: &TempVmState) -> Result<(), StateError> {
        // Validate container name is not empty
        if state.container_name.trim().is_empty() {
            return Err(StateError::ValidationFailed {
                reason: "Container name cannot be empty".to_string(),
            });
        }

        // Validate provider is not empty
        if state.provider.trim().is_empty() {
            return Err(StateError::ValidationFailed {
                reason: "Provider cannot be empty".to_string(),
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

// Note: Default impl intentionally panics for invalid environment
// This ensures early failure detection for misconfigured systems
impl Default for StateManager {
    fn default() -> Self {
        Self::new().expect("Failed to create default state manager - check that home directory is accessible")
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
