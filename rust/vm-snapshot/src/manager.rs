//! Snapshot management and lifecycle operations

use crate::metadata::SnapshotMetadata;
use sha2::{Digest, Sha256};
use std::path::{Component, Path, PathBuf};
use vm_core::error::{Result, VmError};

fn validate_storage_component(value: &str, kind: &str) -> Result<()> {
    let mut components = Path::new(value).components();
    let is_single_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();

    if !is_single_component {
        return Err(VmError::validation(
            format!("Invalid {kind} '{value}': expected a single path component"),
            None::<String>,
        ));
    }

    Ok(())
}

pub(crate) fn snapshot_file_path(base: &Path, name: &str, kind: &str) -> Result<PathBuf> {
    validate_storage_component(name, kind)?;
    Ok(base.join(name))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SnapshotScope<'a> {
    Global,
    Project(&'a str),
    OwnedProject {
        name: &'a str,
        config_path: &'a Path,
    },
}

impl<'a> SnapshotScope<'a> {
    pub fn from_name(name: &'a str, default_project: Option<&'a str>) -> (Self, &'a str) {
        if let Some(stripped) = name.strip_prefix('@') {
            (Self::Global, stripped)
        } else {
            (default_project.map_or(Self::Global, Self::Project), name)
        }
    }

    pub fn project_name(self) -> &'a str {
        match self {
            Self::Global => "global",
            Self::Project(name) => name,
            Self::OwnedProject { name, .. } => name,
        }
    }

    fn storage_key(self) -> Result<String> {
        match self {
            Self::Global => Ok("global".to_string()),
            Self::Project(name) => Ok(name.to_string()),
            Self::OwnedProject { name, config_path } => {
                validate_storage_component(name, "project name")?;
                let canonical = match config_path.canonicalize() {
                    Ok(path) => path,
                    Err(_error)
                        if config_path.is_absolute()
                            && !config_path
                                .components()
                                .any(|component| matches!(component, Component::ParentDir)) =>
                    {
                        config_path.to_path_buf()
                    }
                    Err(error) => {
                        return Err(VmError::filesystem(
                            error,
                            config_path.display(),
                            "canonicalize",
                        ))
                    }
                };
                let path = canonical.to_str().ok_or_else(|| {
                    VmError::validation(
                        "Project configuration path must be UTF-8 for snapshots",
                        None::<String>,
                    )
                })?;
                let digest = Sha256::digest(path.as_bytes());
                let suffix = digest[..12]
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                Ok(format!("{name}-{suffix}"))
            }
        }
    }
}

/// Manages snapshot storage and lifecycle
pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

/// A saved snapshot directory verified against its on-disk metadata.
#[derive(Debug, Clone)]
pub struct SnapshotStorageEntry {
    pub id: String,
    pub project: String,
    pub name: String,
    pub owner_config_path: Option<PathBuf>,
}

impl SnapshotManager {
    /// Stable, owner-scoped identifier used by structured CLI inventory.
    pub fn snapshot_id(&self, scope: SnapshotScope<'_>, name: &str) -> Result<String> {
        validate_storage_component(name, "snapshot name")?;
        Ok(format!("snapshot:{}/{}", scope.storage_key()?, name))
    }

    /// Create a new snapshot manager
    pub fn new() -> Result<Self> {
        let snapshots_dir = vm_core::user_paths::user_config_dir()?.join("snapshots");

        // Create snapshots directory if it doesn't exist
        std::fs::create_dir_all(&snapshots_dir).map_err(|e| {
            VmError::filesystem(e, snapshots_dir.to_string_lossy(), "create_dir_all")
        })?;

        Ok(Self { snapshots_dir })
    }

    /// Get the directory path for a specific snapshot
    pub fn get_snapshot_dir(&self, scope: SnapshotScope<'_>, name: &str) -> Result<PathBuf> {
        validate_storage_component(name, "snapshot name")?;
        let key = scope.storage_key()?;
        validate_storage_component(&key, "project storage key")?;
        Ok(self.snapshots_dir.join(key).join(name))
    }

    pub fn list_snapshots_in_scope(
        &self,
        scope: SnapshotScope<'_>,
    ) -> Result<Vec<SnapshotMetadata>> {
        let mut snapshots = self.list_snapshots(Some(&scope.storage_key()?))?;
        if let SnapshotScope::OwnedProject { name, config_path } = scope {
            let owner = config_path.canonicalize()?.display().to_string();
            snapshots.retain(|snapshot| {
                snapshot.project_name == name
                    && snapshot.owner_config_path.as_deref() == Some(owner.as_str())
            });
        }
        Ok(snapshots)
    }

    /// Create staging beside the final snapshot so installation can use renames.
    pub fn create_staging_dir(
        &self,
        scope: SnapshotScope<'_>,
        name: &str,
    ) -> Result<tempfile::TempDir> {
        let target = self.get_snapshot_dir(scope, name)?;
        let parent = target.parent().ok_or_else(|| {
            VmError::validation("Snapshot target has no parent directory", None::<String>)
        })?;
        std::fs::create_dir_all(parent).map_err(|error| {
            VmError::filesystem(error, parent.to_string_lossy(), "create_dir_all")
        })?;
        tempfile::Builder::new()
            .prefix(".snapshot-staging-")
            .tempdir_in(parent)
            .map_err(|error| VmError::filesystem(error, parent.to_string_lossy(), "tempdir"))
    }

    /// Install a complete staged snapshot, preserving the current snapshot on failure.
    pub fn install_staged_snapshot(
        &self,
        staging: tempfile::TempDir,
        scope: SnapshotScope<'_>,
        name: &str,
        force: bool,
    ) -> Result<()> {
        let target = self.get_snapshot_dir(scope, name)?;
        if target.exists() && !force {
            return Err(VmError::validation(
                format!("Snapshot '{name}' already exists. Use --force to overwrite."),
                None::<String>,
            ));
        }

        if !staging.path().join("metadata.json").is_file() {
            return Err(VmError::validation(
                "Staged snapshot is missing metadata.json",
                None::<String>,
            ));
        }

        if !target.exists() {
            std::fs::rename(staging.path(), &target)
                .map_err(|error| VmError::filesystem(error, target.to_string_lossy(), "rename"))?;
            return Ok(());
        }

        let parent = target.parent().ok_or_else(|| {
            VmError::validation("Snapshot target has no parent directory", None::<String>)
        })?;
        let backup = tempfile::Builder::new()
            .prefix(".snapshot-previous-")
            .tempdir_in(parent)
            .map_err(|error| VmError::filesystem(error, parent.to_string_lossy(), "tempdir"))?;
        let backup_path = backup.path().to_path_buf();
        backup.close().map_err(|error| {
            VmError::filesystem(error, backup_path.to_string_lossy(), "remove_dir_all")
        })?;

        std::fs::rename(&target, &backup_path)
            .map_err(|error| VmError::filesystem(error, target.to_string_lossy(), "rename"))?;
        if let Err(error) = std::fs::rename(staging.path(), &target) {
            if let Err(recovery_error) = std::fs::rename(&backup_path, &target) {
                return Err(VmError::general(
                    recovery_error,
                    format!("Failed to install snapshot '{name}' and recover its previous version"),
                ));
            }
            return Err(VmError::filesystem(
                error,
                target.to_string_lossy(),
                "rename",
            ));
        }

        if let Err(error) = std::fs::remove_dir_all(&backup_path) {
            tracing::warn!(
                snapshot = name,
                path = %backup_path.display(),
                %error,
                "snapshot was replaced but its previous copy could not be removed"
            );
        }

        Ok(())
    }

    /// List all snapshots, optionally filtered by project
    pub fn list_snapshots(&self, project_filter: Option<&str>) -> Result<Vec<SnapshotMetadata>> {
        let mut snapshots = Vec::new();

        // Determine which directories to scan
        let scan_dirs: Vec<PathBuf> = if let Some(project) = project_filter {
            validate_storage_component(project, "project name")?;
            vec![self.snapshots_dir.join(project)]
        } else {
            // Scan all project directories
            let read_dir = std::fs::read_dir(&self.snapshots_dir).map_err(|e| {
                VmError::filesystem(e, self.snapshots_dir.to_string_lossy(), "read_dir")
            })?;

            let mut directories = Vec::new();
            for entry in read_dir {
                let entry = entry.map_err(|error| {
                    VmError::filesystem(error, self.snapshots_dir.display(), "read_dir entry")
                })?;
                let file_type = entry.file_type().map_err(|error| {
                    VmError::filesystem(error, entry.path().display(), "file_type")
                })?;
                if file_type.is_dir() {
                    directories.push(entry.path());
                }
            }
            directories
        };

        // Scan each project directory
        for project_dir in scan_dirs {
            if !project_dir.exists() {
                continue;
            }

            let read_dir = std::fs::read_dir(&project_dir)
                .map_err(|e| VmError::filesystem(e, project_dir.to_string_lossy(), "read_dir"))?;

            for entry in read_dir {
                let entry = entry.map_err(|error| {
                    VmError::filesystem(error, project_dir.display(), "read_dir entry")
                })?;
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                if file_name.starts_with(".snapshot-staging-")
                    || file_name.starts_with(".snapshot-previous-")
                {
                    continue;
                }
                let snapshot_dir = entry.path();
                if !entry
                    .file_type()
                    .map_err(|error| {
                        VmError::filesystem(error, snapshot_dir.display(), "file_type")
                    })?
                    .is_dir()
                {
                    continue;
                }

                let metadata_file = snapshot_dir.join("metadata.json");
                if !metadata_file.exists() {
                    continue;
                }

                match SnapshotMetadata::load(&metadata_file) {
                    Ok(metadata) => snapshots.push(metadata),
                    Err(error) => tracing::warn!(
                        path = %metadata_file.display(),
                        %error,
                        "failed to load snapshot metadata"
                    ),
                }
            }
        }

        // Sort by creation time, newest first
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.created_at));

        Ok(snapshots)
    }

    /// Inventory retained snapshot files without claiming unrelated directories.
    /// Symlinked entries and inconsistent metadata are deliberately omitted.
    pub fn storage_inventory(&self) -> Result<Vec<SnapshotStorageEntry>> {
        let mut entries = Vec::new();
        for project in std::fs::read_dir(&self.snapshots_dir)? {
            let project = project?;
            if !project.file_type()?.is_dir() {
                continue;
            }
            let Some(project_name) = project.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if validate_storage_component(&project_name, "project name").is_err() {
                continue;
            }
            for snapshot in std::fs::read_dir(project.path())? {
                let snapshot = snapshot?;
                if !snapshot.file_type()?.is_dir() {
                    continue;
                }
                let Some(name) = snapshot.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if validate_storage_component(&name, "snapshot name").is_err() {
                    continue;
                }
                let metadata = match SnapshotMetadata::load(snapshot.path().join("metadata.json")) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };
                let expected_key = match metadata.owner_config_path.as_deref() {
                    Some(path) => SnapshotScope::OwnedProject {
                        name: &metadata.project_name,
                        config_path: Path::new(path),
                    }
                    .storage_key()
                    .ok(),
                    None if metadata.project_name == "global" => Some("global".to_string()),
                    None => None,
                };
                if metadata.name != name || expected_key.as_deref() != Some(&project_name) {
                    continue;
                }
                entries.push(SnapshotStorageEntry {
                    id: format!("snapshot:{project_name}/{name}"),
                    project: metadata.project_name,
                    name,
                    owner_config_path: metadata.owner_config_path.map(PathBuf::from),
                });
            }
        }
        entries.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(entries)
    }

    /// Delete a snapshot
    pub fn delete_snapshot(&self, scope: SnapshotScope<'_>, name: &str) -> Result<()> {
        let snapshot_dir = self.get_snapshot_dir(scope, name)?;

        if !snapshot_dir.exists() {
            return Err(VmError::validation(
                format!("Snapshot '{}' not found", name),
                None::<String>,
            ));
        }

        std::fs::remove_dir_all(&snapshot_dir).map_err(|e| {
            VmError::filesystem(e, snapshot_dir.to_string_lossy(), "remove_dir_all")
        })?;

        Ok(())
    }

    /// Check if a snapshot exists
    pub fn snapshot_exists(&self, scope: SnapshotScope<'_>, name: &str) -> Result<bool> {
        let snapshot_dir = self.get_snapshot_dir(scope, name)?;
        Ok(snapshot_dir.exists() && snapshot_dir.join("metadata.json").exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(root: &Path) -> SnapshotManager {
        SnapshotManager {
            snapshots_dir: root.to_path_buf(),
        }
    }

    #[test]
    fn snapshot_paths_stay_within_storage_root() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager(tempdir.path());

        let path = manager
            .get_snapshot_dir(SnapshotScope::Project("demo"), "before-upgrade")
            .unwrap();

        assert_eq!(path, tempdir.path().join("demo/before-upgrade"));
    }

    #[test]
    fn snapshot_paths_reject_traversal_and_absolute_components() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager(tempdir.path());

        for name in ["", ".", "..", "../outside", "/tmp/outside"] {
            assert!(manager
                .get_snapshot_dir(SnapshotScope::Project("demo"), name)
                .is_err());
        }
        assert!(manager
            .get_snapshot_dir(SnapshotScope::Project("../outside"), "snapshot")
            .is_err());
    }

    #[test]
    fn snapshot_metadata_files_are_single_components() {
        let root = Path::new("/snapshots/demo");

        assert_eq!(
            snapshot_file_path(root, "image.tar", "image file").unwrap(),
            root.join("image.tar")
        );
        assert!(snapshot_file_path(root, "../image.tar", "image file").is_err());
        assert!(snapshot_file_path(root, "/tmp/image.tar", "image file").is_err());
    }

    #[test]
    fn staged_install_replaces_only_after_staging_is_complete() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager(tempdir.path());
        let scope = SnapshotScope::Project("demo");
        let target = manager.get_snapshot_dir(scope, "release").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("metadata.json"), "old").unwrap();

        let staging = manager.create_staging_dir(scope, "release").unwrap();
        std::fs::write(staging.path().join("metadata.json"), "new").unwrap();
        manager
            .install_staged_snapshot(staging, scope, "release", true)
            .unwrap();

        assert_eq!(
            std::fs::read_to_string(target.join("metadata.json")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_dir(target.parent().unwrap()).unwrap().count(),
            1
        );
    }

    #[test]
    fn staged_install_without_force_preserves_existing_snapshot() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager(tempdir.path());
        let scope = SnapshotScope::Project("demo");
        let target = manager.get_snapshot_dir(scope, "release").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("metadata.json"), "old").unwrap();

        let staging = manager.create_staging_dir(scope, "release").unwrap();
        std::fs::write(staging.path().join("metadata.json"), "new").unwrap();
        assert!(manager
            .install_staged_snapshot(staging, scope, "release", false)
            .is_err());

        assert_eq!(
            std::fs::read_to_string(target.join("metadata.json")).unwrap(),
            "old"
        );
    }

    #[test]
    fn incomplete_staging_preserves_existing_snapshot() {
        let tempdir = tempfile::tempdir().unwrap();
        let manager = manager(tempdir.path());
        let scope = SnapshotScope::Project("demo");
        let target = manager.get_snapshot_dir(scope, "release").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("metadata.json"), "old").unwrap();

        let staging = manager.create_staging_dir(scope, "release").unwrap();
        assert!(manager
            .install_staged_snapshot(staging, scope, "release", true)
            .is_err());

        assert_eq!(
            std::fs::read_to_string(target.join("metadata.json")).unwrap(),
            "old"
        );
    }

    #[test]
    fn storage_inventory_requires_matching_metadata_and_plain_directories() {
        let root = tempfile::tempdir().unwrap();
        let manager = manager(root.path());
        let owner_dir = root.path().join("owner");
        std::fs::create_dir(&owner_dir).unwrap();
        let owner = owner_dir.join("vm.yaml");
        std::fs::write(&owner, "project: demo").unwrap();
        let project = manager
            .get_snapshot_dir(
                SnapshotScope::OwnedProject {
                    name: "demo",
                    config_path: &owner,
                },
                "stable",
            )
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let valid = project.join("stable");
        let mismatch = project.join("wrong");
        std::fs::create_dir_all(&valid).unwrap();
        std::fs::create_dir_all(&mismatch).unwrap();
        let metadata = serde_json::json!({
            "name":"stable","created_at":"2026-09-25T00:00:00Z",
            "description":null,"project_name":"demo","source_environment":"demo-dev",
            "provider":"tart","architecture":"aarch64","consistency":"stopped",
            "project_dir":owner_dir.display().to_string(),"owner_config_path":owner.display().to_string(),
            "git_commit":null,"git_dirty":false,"git_branch":null,
            "services":[],"volumes":[],"excluded_mounts":[],
            "compose_file":"","vm_config_file":"","total_size_bytes":1
        });
        let encoded = serde_json::to_vec(&metadata).unwrap();
        std::fs::write(valid.join("metadata.json"), &encoded).unwrap();
        std::fs::write(mismatch.join("metadata.json"), &encoded).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&valid, project.join("linked")).unwrap();

        let resources = manager.storage_inventory().unwrap();
        assert_eq!(resources.len(), 1);
        assert_eq!(
            resources[0].id,
            format!(
                "snapshot:{}/stable",
                project.file_name().unwrap().to_string_lossy()
            )
        );
        assert_eq!(
            resources[0].owner_config_path.as_deref(),
            Some(owner.as_path())
        );
    }

    #[test]
    fn same_named_projects_have_separate_snapshot_roots() {
        let root = tempfile::tempdir().unwrap();
        let manager = manager(root.path());
        let first = root.path().join("one/vm.yaml");
        let second = root.path().join("two/vm.yaml");
        std::fs::create_dir_all(first.parent().unwrap()).unwrap();
        std::fs::create_dir_all(second.parent().unwrap()).unwrap();
        std::fs::write(&first, "project: demo").unwrap();
        std::fs::write(&second, "project: demo").unwrap();
        let first_path = manager
            .get_snapshot_dir(
                SnapshotScope::OwnedProject {
                    name: "demo",
                    config_path: &first,
                },
                "stable",
            )
            .unwrap();
        let second_path = manager
            .get_snapshot_dir(
                SnapshotScope::OwnedProject {
                    name: "demo",
                    config_path: &second,
                },
                "stable",
            )
            .unwrap();
        assert_ne!(first_path, second_path);
    }
}
