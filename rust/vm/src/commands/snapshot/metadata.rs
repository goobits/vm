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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_serialization_roundtrip() {
        let metadata = SnapshotMetadata {
            name: "test-snapshot".to_string(),
            created_at: Utc::now(),
            description: Some("Test snapshot for verification".to_string()),
            project_name: "myproject".to_string(),
            project_dir: "/workspace/myproject".to_string(),
            git_commit: Some("abc123def456".to_string()),
            git_dirty: true,
            git_branch: Some("main".to_string()),
            services: vec![ServiceSnapshot {
                name: "web".to_string(),
                image_tag: "vm-snapshot/myproject/web:test".to_string(),
                image_file: "web.tar".to_string(),
                image_digest: Some("sha256:1234567890abcdef".to_string()),
            }],
            volumes: vec![VolumeSnapshot {
                name: "postgres_data".to_string(),
                archive_file: "postgres_data.tar.gz".to_string(),
                size_bytes: 1048576,
            }],
            compose_file: "docker-compose.yml".to_string(),
            vm_config_file: "vm.yaml".to_string(),
            total_size_bytes: 2097152,
        };

        // Serialize to JSON
        let json = serde_json::to_string(&metadata).unwrap();

        // Deserialize back
        let restored: SnapshotMetadata = serde_json::from_str(&json).unwrap();

        // Verify critical fields survived the roundtrip
        assert_eq!(metadata.name, restored.name);
        assert_eq!(metadata.project_name, restored.project_name);
        assert_eq!(metadata.project_dir, restored.project_dir);
        assert_eq!(metadata.git_commit, restored.git_commit);
        assert_eq!(metadata.git_dirty, restored.git_dirty);
        assert_eq!(metadata.git_branch, restored.git_branch);
        assert_eq!(metadata.compose_file, restored.compose_file);
        assert_eq!(metadata.vm_config_file, restored.vm_config_file);
        assert_eq!(metadata.total_size_bytes, restored.total_size_bytes);
        assert_eq!(metadata.services.len(), restored.services.len());
        assert_eq!(metadata.volumes.len(), restored.volumes.len());

        // Verify nested structures
        assert_eq!(metadata.services[0].name, restored.services[0].name);
        assert_eq!(
            metadata.services[0].image_tag,
            restored.services[0].image_tag
        );
        assert_eq!(metadata.volumes[0].name, restored.volumes[0].name);
        assert_eq!(
            metadata.volumes[0].size_bytes,
            restored.volumes[0].size_bytes
        );
    }

    #[test]
    fn test_metadata_backward_compatibility() {
        // Simulates loading an old snapshot created before potential field additions
        let json = r#"{
            "name": "old-snapshot",
            "created_at": "2024-01-01T00:00:00Z",
            "description": null,
            "project_name": "legacy-project",
            "project_dir": "/workspace/legacy",
            "git_commit": null,
            "git_dirty": false,
            "git_branch": null,
            "services": [],
            "volumes": [],
            "compose_file": "docker-compose.yml",
            "vm_config_file": "vm.yaml",
            "total_size_bytes": 0
        }"#;

        let metadata: SnapshotMetadata = serde_json::from_str(json).unwrap();

        assert_eq!(metadata.name, "old-snapshot");
        assert_eq!(metadata.project_name, "legacy-project");
        assert_eq!(metadata.description, None);
        assert_eq!(metadata.git_commit, None);
    }
}
