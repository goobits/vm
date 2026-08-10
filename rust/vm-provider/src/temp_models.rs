use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use vm_config::config::mounts::{resolve_mount_source, validate_mount_target};
use vm_config::config::MountConfig;
use vm_core::error::{Result, VmError};

pub use vm_config::config::MountAccess as MountPermission;

/// Represents a single mount point in a temp VM
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Mount {
    /// Source path on the host system
    pub source: PathBuf,
    /// Target path inside the VM
    pub target: PathBuf,
    /// Mount permissions
    #[serde(serialize_with = "serialize_mount_permission")]
    pub permissions: MountPermission,
}

fn serialize_mount_permission<S>(
    permission: &MountPermission,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(permission.as_mode())
}

impl Mount {
    fn default_target_for_source(source: &Path) -> PathBuf {
        let basename = source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mounted");
        PathBuf::from("/workspace").join(basename)
    }

    /// Create a new mount with the given source and permissions
    /// Target path is automatically generated as /workspace/{basename}
    pub fn new(source: PathBuf, permissions: MountPermission) -> Self {
        Self {
            target: Self::default_target_for_source(&source),
            source,
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

    /// Resolve a declarative mount relative to the project configuration.
    pub fn from_config(config: &MountConfig, project_dir: &Path) -> Result<Self> {
        Ok(Self::with_target(
            config.resolved_source(project_dir)?,
            config.target.clone(),
            config.access,
        ))
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
        let target = Mount::default_target_for_source(&source);
        let source = Self::canonical_mount_source(&source)?;

        if self.has_mount(&source) {
            return Err(VmError::Config(format!(
                "Mount already exists for source: {}",
                source.display()
            )));
        }

        let mount = Mount::with_target(source, target, permissions);
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
        let source = Self::canonical_mount_source(&source)?;

        Self::validate_target_path(&target)?;

        if self.has_mount(&source) {
            return Err(VmError::Config(format!(
                "Mount already exists for source: {}",
                source.display()
            )));
        }

        let mount = Mount::with_target(source, target, permissions);
        self.mounts.push(mount);

        Ok(())
    }

    /// Remove a mount by source path
    pub fn remove_mount(&mut self, source: &Path) -> Result<Mount> {
        let source = Self::normalize_mount_lookup(source);
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
        let source = Self::normalize_mount_lookup(source);
        self.mounts.iter().any(|mount| mount.source == source)
    }

    /// Get a mount by source path
    pub fn get_mount(&self, source: &Path) -> Option<&Mount> {
        let source = Self::normalize_mount_lookup(source);
        self.mounts.iter().find(|mount| mount.source == source)
    }

    /// Get a mutable reference to a mount by source path
    pub fn get_mount_mut(&mut self, source: &Path) -> Option<&mut Mount> {
        let source = Self::normalize_mount_lookup(source);
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

    fn canonical_mount_source(source: &Path) -> Result<PathBuf> {
        let current_dir = std::env::current_dir().map_err(|error| {
            VmError::Internal(format!("Failed to determine current directory: {error}"))
        })?;
        resolve_mount_source(source, &current_dir)
    }

    fn normalize_mount_lookup(source: &Path) -> PathBuf {
        source
            .canonicalize()
            .unwrap_or_else(|_| source.to_path_buf())
    }

    /// Validate a target path for mounting
    fn validate_target_path(target: &Path) -> Result<()> {
        validate_mount_target(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn add_mount_stores_canonical_source() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        std::fs::create_dir(&source).unwrap();

        let mut state = TempVmState::new(
            "temp".to_string(),
            "docker".to_string(),
            tmp.path().to_path_buf(),
            false,
        );
        state
            .add_mount(source.clone(), MountPermission::ReadOnly)
            .unwrap();

        assert_eq!(state.mounts[0].source, source.canonicalize().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn mount_source_rejects_symlink_to_dangerous_path() {
        let tmp = tempfile::tempdir().unwrap();
        let link = tmp.path().join("etc-link");
        std::os::unix::fs::symlink("/etc", &link).unwrap();

        assert!(TempVmState::canonical_mount_source(&link).is_err());
    }
}
