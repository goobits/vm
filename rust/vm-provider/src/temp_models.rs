use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use vm_core::error::{Result, VmError};

/// Mount permission levels for temp VM mounts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MountPermission {
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    #[default]
    ReadWrite,
}

impl std::fmt::Display for MountPermission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountPermission::ReadOnly => write!(f, "ro"),
            MountPermission::ReadWrite => write!(f, "rw"),
        }
    }
}

impl std::str::FromStr for MountPermission {
    type Err = VmError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "ro" => Ok(MountPermission::ReadOnly),
            "rw" => Ok(MountPermission::ReadWrite),
            _ => Err(VmError::Internal(format!(
                "Invalid permission '{s}'. Use 'ro' or 'rw'"
            ))),
        }
    }
}

/// Represents a single mount point in a temp VM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mount {
    /// Source path on the host system
    pub source: PathBuf,
    /// Target path inside the VM
    pub target: PathBuf,
    /// Mount permissions
    pub permissions: MountPermission,
}

impl Mount {
    /// Create a new mount with the given source and permissions
    /// Target path is automatically generated as /workspace/{basename}
    pub fn new(source: PathBuf, permissions: MountPermission) -> Self {
        let basename = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mounted");
        let target = PathBuf::from("/workspace").join(basename);

        Self {
            source,
            target,
            permissions,
        }
    }

    /// Create a new mount with custom target path
    pub fn with_target(source: PathBuf, target: PathBuf, permissions: MountPermission) -> Self {
        Self {
            source,
            target,
            permissions,
        }
    }

    /// Get the mount string for provider use (source:target:permissions)
    pub fn to_mount_string(&self) -> String {
        format!(
            "{}:{}:{}",
            self.source.display(),
            self.target.display(),
            self.permissions
        )
    }
}

/// Complete state of a temporary VM instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempVmState {
    /// Container/VM name for provider operations
    pub container_name: String,
    /// VM provider being used (docker, tart, podman, etc.)
    pub provider: String,
    /// List of mounted directories
    pub mounts: Vec<Mount>,
    /// When the VM was created
    pub created_at: DateTime<Utc>,
    /// Project directory from which the VM was created
    pub project_dir: PathBuf,
    /// Whether the VM should auto-destroy after SSH session
    pub auto_destroy: bool,
}

impl TempVmState {
    /// Create a new temp VM state
    pub fn new(
        container_name: String,
        provider: String,
        project_dir: PathBuf,
        auto_destroy: bool,
    ) -> Self {
        Self {
            container_name,
            provider,
            mounts: Vec::new(),
            created_at: Utc::now(),
            project_dir,
            auto_destroy,
        }
    }

    /// Get the number of mounts
    pub fn mount_count(&self) -> usize {
        self.mounts.len()
    }

    /// Check if the VM is configured for auto-destruction
    pub fn is_auto_destroy(&self) -> bool {
        self.auto_destroy
    }

    /// Get all mount strings for provider use
    pub fn mount_strings(&self) -> Vec<String> {
        self.mounts
            .iter()
            .map(|mount| mount.to_mount_string())
            .collect()
    }

    /// Add a new mount to the temp VM
    pub fn add_mount(&mut self, source: PathBuf, permissions: MountPermission) -> Result<()> {
        // Validate the mount source
        Self::validate_mount_source(&source)?;

        // Check if mount already exists
        if self.has_mount(&source) {
            return Err(VmError::Config(format!(
                "Mount already exists for source: {}",
                source.display()
            )));
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
    ) -> Result<()> {
        // Validate the mount source
        Self::validate_mount_source(&source)?;

        // Validate the target path
        Self::validate_target_path(&target)?;

        // Check if mount already exists
        if self.has_mount(&source) {
            return Err(VmError::Config(format!(
                "Mount already exists for source: {}",
                source.display()
            )));
        }

        // Create the mount
        let mount = Mount::with_target(source, target, permissions);
        self.mounts.push(mount);

        Ok(())
    }

    /// Remove a mount by source path
    pub fn remove_mount(&mut self, source: &Path) -> Result<Mount> {
        let index = self
            .mounts
            .iter()
            .position(|mount| mount.source == source)
            .ok_or_else(|| {
                VmError::Config(format!("Mount not found for source: {}", source.display()))
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
    ) -> Result<()> {
        let mount = self.get_mount_mut(source).ok_or_else(|| {
            VmError::Config(format!("Mount not found for source: {}", source.display()))
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
    fn validate_mount_source(source: &Path) -> Result<()> {
        // Check if source exists
        if !source.exists() {
            return Err(VmError::Config(format!(
                "Mount source does not exist: {}",
                source.display()
            )));
        }

        // Check if source is a directory
        if !source.is_dir() {
            return Err(VmError::Config(format!(
                "Mount source is not a directory: {}",
                source.display()
            )));
        }

        // Security check: prevent mounting dangerous system directories
        if Self::is_dangerous_mount_path(source) {
            return Err(VmError::Config(format!(
                "Dangerous mount path not allowed: {}",
                source.display()
            )));
        }

        Ok(())
    }

    /// Validate a target path for mounting
    fn validate_target_path(target: &Path) -> Result<()> {
        // Target should be absolute and under /workspace or /tmp
        if !target.is_absolute() {
            return Err(VmError::Config(format!(
                "Invalid target path: {}",
                target.display()
            )));
        }

        // Check if target is under allowed directories
        let allowed_prefixes = ["/workspace", "/tmp", "/home"];
        let target_str = target.to_string_lossy();

        if !allowed_prefixes
            .iter()
            .any(|prefix| target_str.starts_with(prefix))
        {
            return Err(VmError::Config(format!(
                "Invalid target path: {}",
                target.display()
            )));
        }

        Ok(())
    }

    /// Check if a path is dangerous to mount (system directories)
    fn is_dangerous_mount_path(path: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dangerous_path_detection() {
        assert!(TempVmState::is_dangerous_mount_path(Path::new("/")));
        assert!(TempVmState::is_dangerous_mount_path(Path::new("/etc")));
        assert!(TempVmState::is_dangerous_mount_path(Path::new(
            "/etc/nginx"
        )));
        assert!(TempVmState::is_dangerous_mount_path(Path::new("/usr/bin")));

        assert!(!TempVmState::is_dangerous_mount_path(Path::new(
            "/home/user"
        )));
        assert!(!TempVmState::is_dangerous_mount_path(Path::new("/tmp")));
        assert!(!TempVmState::is_dangerous_mount_path(Path::new(
            "/workspace"
        )));
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
