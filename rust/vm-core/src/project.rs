//! Package storage utilities
//!
//! Provides functions for locating global package storage directories.

use crate::error::{Result, VmError};
use std::path::PathBuf;

/// Get the global data directory for package storage
///
/// Returns `~/.vm/packages` as a shared global storage location for all
/// package registry operations across all projects and VMs.
///
/// # Returns
/// - `Ok(PathBuf)` - Path to global packages directory
/// - `Err(VmError)` - If unable to determine home directory
///
/// # Examples
/// ```
/// use vm_core::project::get_package_data_dir;
///
/// let data_dir = get_package_data_dir()?;
/// // data_dir = /home/user/.vm/packages (Linux)
/// // data_dir = /Users/user/.vm/packages (macOS)
/// // data_dir = C:\Users\user\.vm\packages (Windows)
/// # Ok::<(), vm_core::error::VmError>(())
/// ```
pub fn get_package_data_dir() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| VmError::Config("Cannot determine home directory".to_string()))?;
    Ok(home.join(".vm").join("packages"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_package_data_dir() {
        let data_dir = get_package_data_dir().unwrap();

        // Should return ~/.vm/packages
        assert!(data_dir.ends_with(".vm/packages") || data_dir.ends_with(r".vm\packages"));

        // Should be an absolute path
        assert!(data_dir.is_absolute());
    }
}
