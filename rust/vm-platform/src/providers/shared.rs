//! Shared implementations for platform providers.
//!
//! This module contains default implementations of PlatformProvider methods
//! that are identical across all platforms. Platform-specific implementations
//! can use these or override them as needed.

use crate::traits::PlatformProvider;
use anyhow::{Context, Result};
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

/// Provides default implementations for common path operations
pub trait SharedPlatformOps: PlatformProvider {
    #[cfg(unix)]
    fn install_symlink(&self, source: &Path, dest_dir: &Path, name: &str) -> Result<()> {
        std::fs::create_dir_all(dest_dir).context("Failed to create destination directory")?;
        let dest = dest_dir.join(name);
        if dest.exists() || dest.is_symlink() {
            std::fs::remove_file(&dest).context("Failed to remove existing file/symlink")?;
        }
        std::os::unix::fs::symlink(source, &dest).context("Failed to create symlink")
    }
    /// Default implementation for user_config_dir
    fn default_user_config_dir(&self) -> Result<PathBuf> {
        Ok(dirs::config_dir()
            .context("Could not determine user config directory")?
            .join("vm"))
    }

    /// Default implementation for user_data_dir
    fn default_user_data_dir(&self) -> Result<PathBuf> {
        Ok(dirs::data_dir()
            .context("Could not determine user data directory")?
            .join("vm"))
    }

    /// Default implementation for home_dir
    fn default_home_dir(&self) -> Result<PathBuf> {
        dirs::home_dir().context("Could not determine home directory")
    }

    /// Default implementation for documents_dir
    fn default_documents_dir(&self) -> Result<PathBuf> {
        dirs::document_dir().context("Could not determine documents directory")
    }

    /// Default implementation for vm_state_dir
    fn default_vm_state_dir(&self) -> Result<PathBuf> {
        Ok(self.home_dir()?.join(".vm"))
    }

    /// Default implementation for cargo_home
    fn default_cargo_home(&self) -> Result<PathBuf> {
        if let Ok(cargo_home) = std::env::var("CARGO_HOME") {
            Ok(PathBuf::from(cargo_home))
        } else {
            Ok(self.home_dir()?.join(".cargo"))
        }
    }

    /// Default implementation for cargo_bin_dir
    fn default_cargo_bin_dir(&self) -> Result<PathBuf> {
        Ok(self.cargo_home()?.join("bin"))
    }

    /// Default implementation for path_separator (Unix-like systems)
    fn default_path_separator(&self) -> char {
        ':'
    }

    /// Default implementation for split_path_env
    fn default_split_path_env(&self, path: &str) -> Vec<PathBuf> {
        std::env::split_paths(path).collect()
    }

    /// Default implementation for join_path_env
    fn default_join_path_env(&self, paths: &[PathBuf]) -> String {
        std::env::join_paths(paths)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}
