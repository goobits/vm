use crate::{Mount, MountPermission, TempVmState};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MountError {
    #[error("Mount source does not exist: {path}")]
    SourceNotFound { path: PathBuf },
    #[error("Mount source is not a directory: {path}")]
    SourceNotDirectory { path: PathBuf },
    #[error("Dangerous mount path not allowed: {path}")]
    DangerousPath { path: PathBuf },
    #[error("Mount already exists for source: {path}")]
    MountExists { path: PathBuf },
    #[error("Mount not found for source: {path}")]
    MountNotFound { path: PathBuf },
    #[error("Invalid target path: {path}")]
    InvalidTarget { path: PathBuf },
}

/// Mount operations for temp VM state
impl TempVmState {
    /// Add a new mount to the temp VM
    pub fn add_mount(&mut self, source: PathBuf, permissions: MountPermission) -> Result<(), MountError> {
        // Validate the mount source
        Self::validate_mount_source(&source)?;

        // Check if mount already exists
        if self.has_mount(&source) {
            return Err(MountError::MountExists { path: source });
        }

        // Create the mount
        let mount = Mount::new(source, permissions);
        self.mounts.push(mount);

        Ok(())
    }

    /// Add a mount with a custom target path
    pub fn add_mount_with_target(
        &mut self,
        source: PathBuf,
        target: PathBuf,
        permissions: MountPermission,
    ) -> Result<(), MountError> {
        // Validate the mount source
        Self::validate_mount_source(&source)?;

        // Validate the target path
        Self::validate_target_path(&target)?;

        // Check if mount already exists
        if self.has_mount(&source) {
            return Err(MountError::MountExists { path: source });
        }

        // Create the mount
        let mount = Mount::with_target(source, target, permissions);
        self.mounts.push(mount);

        Ok(())
    }

    /// Remove a mount by source path
    pub fn remove_mount(&mut self, source: &Path) -> Result<Mount, MountError> {
        let index = self
            .mounts
            .iter()
            .position(|mount| mount.source == source)
            .ok_or_else(|| MountError::MountNotFound {
                path: source.to_path_buf(),
            })?;

        Ok(self.mounts.remove(index))
    }

    /// Check if a mount exists for the given source path
    pub fn has_mount(&self, source: &Path) -> bool {
        self.mounts.iter().any(|mount| mount.source == source)
    }

    /// Get a mount by source path
    pub fn get_mount(&self, source: &Path) -> Option<&Mount> {
        self.mounts.iter().find(|mount| mount.source == source)
    }

    /// Get a mutable reference to a mount by source path
    pub fn get_mount_mut(&mut self, source: &Path) -> Option<&mut Mount> {
        self.mounts.iter_mut().find(|mount| mount.source == source)
    }

    /// Get all mounts
    pub fn get_mounts(&self) -> &[Mount] {
        &self.mounts
    }

    /// Clear all mounts
    pub fn clear_mounts(&mut self) {
        self.mounts.clear();
    }

    /// Update mount permissions for an existing mount
    pub fn update_mount_permissions(
        &mut self,
        source: &Path,
        permissions: MountPermission,
    ) -> Result<(), MountError> {
        let mount = self.get_mount_mut(source).ok_or_else(|| MountError::MountNotFound {
            path: source.to_path_buf(),
        })?;

        mount.permissions = permissions;
        Ok(())
    }

    /// Get mounts by permission type
    pub fn get_mounts_by_permission(&self, permission: MountPermission) -> Vec<&Mount> {
        self.mounts
            .iter()
            .filter(|mount| mount.permissions == permission)
            .collect()
    }

    /// Get mount count by permission type
    pub fn mount_count_by_permission(&self, permission: MountPermission) -> usize {
        self.mounts
            .iter()
            .filter(|mount| mount.permissions == permission)
            .count()
    }

    /// Validate a mount source path
    fn validate_mount_source(source: &Path) -> Result<(), MountError> {
        // Check if source exists
        if !source.exists() {
            return Err(MountError::SourceNotFound {
                path: source.to_path_buf(),
            });
        }

        // Check if source is a directory
        if !source.is_dir() {
            return Err(MountError::SourceNotDirectory {
                path: source.to_path_buf(),
            });
        }

        // Security check: prevent mounting dangerous system directories
        if Self::is_dangerous_mount_path(source) {
            return Err(MountError::DangerousPath {
                path: source.to_path_buf(),
            });
        }

        Ok(())
    }

    /// Validate a target path for mounting
    fn validate_target_path(target: &Path) -> Result<(), MountError> {
        // Target should be absolute and under /workspace or /tmp
        if !target.is_absolute() {
            return Err(MountError::InvalidTarget {
                path: target.to_path_buf(),
            });
        }

        // Check if target is under allowed directories
        let allowed_prefixes = ["/workspace", "/tmp", "/home"];
        let target_str = target.to_string_lossy();

        if !allowed_prefixes.iter().any(|prefix| target_str.starts_with(prefix)) {
            return Err(MountError::InvalidTarget {
                path: target.to_path_buf(),
            });
        }

        Ok(())
    }

    /// Check if a path is dangerous to mount (system directories)
    fn is_dangerous_mount_path(path: &Path) -> bool {
        let dangerous_paths = [
            "/",
            "/etc",
            "/usr",
            "/var",
            "/bin",
            "/sbin",
            "/boot",
            "/sys",
            "/proc",
            "/dev",
            "/root",
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

/// Mount parsing utilities
pub struct MountParser;

impl MountParser {
    /// Parse mount string in format "source:permissions" or "source:target:permissions"
    pub fn parse_mount_string(mount_str: &str) -> Result<(PathBuf, Option<PathBuf>, MountPermission)> {
        let parts: Vec<&str> = mount_str.split(':').collect();

        match parts.len() {
            1 => {
                // Just source path, use default permissions
                let source = PathBuf::from(parts[0]);
                Ok((source, None, MountPermission::default()))
            }
            2 => {
                // source:permissions
                let source = PathBuf::from(parts[0]);
                let permissions = parts[1].parse::<MountPermission>()
                    .with_context(|| format!("Invalid permission in mount string: {}", mount_str))?;
                Ok((source, None, permissions))
            }
            3 => {
                // source:target:permissions
                let source = PathBuf::from(parts[0]);
                let target = PathBuf::from(parts[1]);
                let permissions = parts[2].parse::<MountPermission>()
                    .with_context(|| format!("Invalid permission in mount string: {}", mount_str))?;
                Ok((source, Some(target), permissions))
            }
            _ => Err(anyhow::anyhow!(
                "Invalid mount string format: {}. Expected 'source', 'source:permissions', or 'source:target:permissions'",
                mount_str
            )),
        }
    }

    /// Parse multiple mount strings
    pub fn parse_mount_strings(mount_strings: &[String]) -> Result<Vec<(PathBuf, Option<PathBuf>, MountPermission)>> {
        mount_strings
            .iter()
            .map(|s| Self::parse_mount_string(s))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_parser() {
        // Test simple source
        let (source, target, perm) = MountParser::parse_mount_string("/home/user").unwrap();
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, None);
        assert_eq!(perm, MountPermission::ReadWrite);

        // Test source with permissions
        let (source, target, perm) = MountParser::parse_mount_string("/home/user:ro").unwrap();
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, None);
        assert_eq!(perm, MountPermission::ReadOnly);

        // Test source with target and permissions
        let (source, target, perm) = MountParser::parse_mount_string("/home/user:/workspace/user:rw").unwrap();
        assert_eq!(source, PathBuf::from("/home/user"));
        assert_eq!(target, Some(PathBuf::from("/workspace/user")));
        assert_eq!(perm, MountPermission::ReadWrite);
    }

    #[test]
    fn test_dangerous_path_detection() {
        assert!(TempVmState::is_dangerous_mount_path(Path::new("/")));
        assert!(TempVmState::is_dangerous_mount_path(Path::new("/etc")));
        assert!(TempVmState::is_dangerous_mount_path(Path::new("/etc/nginx")));
        assert!(TempVmState::is_dangerous_mount_path(Path::new("/usr/bin")));

        assert!(!TempVmState::is_dangerous_mount_path(Path::new("/home/user")));
        assert!(!TempVmState::is_dangerous_mount_path(Path::new("/tmp")));
        assert!(!TempVmState::is_dangerous_mount_path(Path::new("/workspace")));
    }

    #[test]
    fn test_target_path_validation() {
        // Valid targets
        assert!(TempVmState::validate_target_path(Path::new("/workspace/test")).is_ok());
        assert!(TempVmState::validate_target_path(Path::new("/tmp/test")).is_ok());
        assert!(TempVmState::validate_target_path(Path::new("/home/user")).is_ok());

        // Invalid targets
        assert!(TempVmState::validate_target_path(Path::new("relative/path")).is_err());
        assert!(TempVmState::validate_target_path(Path::new("/etc/test")).is_err());
        assert!(TempVmState::validate_target_path(Path::new("/usr/test")).is_err());
    }
}