use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Mount permission levels for temp VM mounts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MountPermission {
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
}

impl Default for MountPermission {
    fn default() -> Self {
        Self::ReadWrite
    }
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
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ro" => Ok(MountPermission::ReadOnly),
            "rw" => Ok(MountPermission::ReadWrite),
            _ => Err(anyhow::anyhow!("Invalid permission '{}'. Use 'ro' or 'rw'", s)),
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
    /// VM provider being used (docker, tart, vagrant, etc.)
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
        self.mounts.iter().map(|mount| mount.to_mount_string()).collect()
    }
}