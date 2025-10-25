//! Snapshot metadata structures and persistence

use crate::error::{VmError, VmResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Complete snapshot metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    /// Snapshot name/identifier
    pub name: String,
    /// When the snapshot was created
    pub created_at: DateTime<Utc>,
    /// Optional user-provided description
    pub description: Option<String>,
    /// Project name from config
    pub project_name: String,
    /// Project directory path at snapshot time
    pub project_dir: String,
    /// Git commit hash at snapshot time
    pub git_commit: Option<String>,
    /// Whether working directory was dirty
    pub git_dirty: bool,
    /// Git branch at snapshot time
    pub git_branch: Option<String>,
    /// Services captured in snapshot
    pub services: Vec<ServiceSnapshot>,
    /// Volumes captured in snapshot
    pub volumes: Vec<VolumeSnapshot>,
    /// Relative path to compose file
    pub compose_file: String,
    /// Relative path to vm config file
    pub vm_config_file: String,
    /// Total size in bytes
    pub total_size_bytes: u64,
}

/// Information about a snapshotted service/container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    /// Service name from docker-compose
    pub name: String,
    /// Docker image tag
    pub image_tag: String,
    /// Filename of saved image archive
    pub image_file: String,
    /// Image digest for verification
    pub image_digest: Option<String>,
}

/// Information about a snapshotted volume
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSnapshot {
    /// Volume name
    pub name: String,
    /// Filename of volume archive
    pub archive_file: String,
    /// Archive size in bytes
    pub size_bytes: u64,
}

impl SnapshotMetadata {
    /// Load metadata from JSON file
    pub fn load<P: AsRef<Path>>(path: P) -> VmResult<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .map_err(|e| VmError::filesystem(e, path.to_string_lossy(), "read"))?;

        let metadata: Self = serde_json::from_str(&content).map_err(|e| {
            VmError::general(
                e,
                format!("Failed to parse snapshot metadata from {}", path.display()),
            )
        })?;

        Ok(metadata)
    }

    /// Save metadata to JSON file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> VmResult<()> {
        let path = path.as_ref();
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| VmError::general(e, "Failed to serialize snapshot metadata"))?;

        std::fs::write(path, content)
            .map_err(|e| VmError::filesystem(e, path.to_string_lossy(), "write"))?;

        Ok(())
    }
}
