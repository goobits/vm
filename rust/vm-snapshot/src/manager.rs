//! Snapshot management and lifecycle operations

use crate::metadata::SnapshotMetadata;
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
        }
    }
}

/// Manages snapshot storage and lifecycle
pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

impl SnapshotManager {
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
        let project = scope.project_name();
        validate_storage_component(project, "project name")?;
        Ok(self.snapshots_dir.join(project).join(name))
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
            vm_core::vm_warning!(
                "Snapshot '{}' was replaced, but its previous copy at '{}' could not be removed: {}",
                name,
                backup_path.display(),
                error
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

            read_dir
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.path())
                .collect()
        };

        // Scan each project directory
        for project_dir in scan_dirs {
            if !project_dir.exists() {
                continue;
            }

            let read_dir = std::fs::read_dir(&project_dir)
                .map_err(|e| VmError::filesystem(e, project_dir.to_string_lossy(), "read_dir"))?;

            for entry in read_dir.filter_map(|e| e.ok()) {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                if file_name.starts_with(".snapshot-staging-")
                    || file_name.starts_with(".snapshot-previous-")
                {
                    continue;
                }
                let snapshot_dir = entry.path();
                if !snapshot_dir.is_dir() {
                    continue;
                }

                let metadata_file = snapshot_dir.join("metadata.json");
                if !metadata_file.exists() {
                    continue;
                }

                match SnapshotMetadata::load(&metadata_file) {
                    Ok(metadata) => snapshots.push(metadata),
                    Err(_) => vm_core::vm_warning!(
                        "Failed to load snapshot metadata from {}",
                        metadata_file.display()
                    ),
                }
            }
        }

        // Sort by creation time, newest first
        snapshots.sort_by_key(|snapshot| std::cmp::Reverse(snapshot.created_at));

        Ok(snapshots)
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

/// Handle the list subcommand
pub async fn handle_list(
    project: Option<&str>,
    snapshot_type: Option<&str>,
    default_global_only: bool,
) -> Result<()> {
    let manager = SnapshotManager::new()?;
    let mut snapshots = manager.list_snapshots(project)?;

    // Filter by type if specified
    let filter_type = if snapshot_type.is_none() && project.is_none() && default_global_only {
        Some("base")
    } else {
        snapshot_type
    };

    if let Some(filter_type) = filter_type {
        snapshots.retain(|snapshot| {
            let is_base = snapshot.project_name == "global";
            match filter_type {
                "base" => is_base,
                "project" => !is_base,
                _ => true,
            }
        });
    }

    if snapshots.is_empty() {
        vm_core::vm_println!("No snapshots found.");
        return Ok(());
    }

    vm_core::vm_println!(
        "{:<9} {:<20} {:<21} {:>10} {:<20}",
        "TYPE",
        "NAME",
        "CREATED",
        "SIZE",
        "DESCRIPTION"
    );
    vm_core::vm_println!("{}", "─".repeat(84));

    for snapshot in snapshots {
        // Determine snapshot type (base or project)
        let snapshot_type = if snapshot.project_name == "global" {
            "base"
        } else {
            "project"
        };

        let size_mb = snapshot.total_size_bytes as f64 / (1024.0 * 1024.0);
        let size_display = format!("{:.1} MB", size_mb);

        vm_core::vm_println!(
            "{:<9} {:<20} {:<21} {:>10} {:<20}",
            snapshot_type,
            truncate_string(&snapshot.name, 20),
            snapshot.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            size_display,
            truncate_string(snapshot.description.as_deref().unwrap_or("--"), 20)
        );
    }

    vm_core::vm_println!("");

    Ok(())
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!(
            "{}...",
            s.chars()
                .take(max_len.saturating_sub(3))
                .collect::<String>()
        )
    }
}

/// Handle the delete subcommand
pub async fn handle_delete(name: &str, project: Option<&str>, force: bool) -> Result<()> {
    let manager = SnapshotManager::new()?;

    let (scope, snapshot_name) = SnapshotScope::from_name(name, project);

    if !manager.snapshot_exists(scope, snapshot_name)? {
        let scope_desc = if matches!(scope, SnapshotScope::Global) {
            "global snapshots".to_string()
        } else if let Some(proj) = project {
            format!("project '{}'", proj)
        } else {
            "current project".to_string()
        };
        return Err(VmError::validation(
            format!("Snapshot '{}' not found in {}", snapshot_name, scope_desc),
            None::<String>,
        ));
    }

    if !force {
        let scope_desc = if matches!(scope, SnapshotScope::Global) {
            " (global)".to_string()
        } else {
            String::new()
        };
        vm_core::vm_println!(
            "This will permanently delete the snapshot '{}'{}.",
            snapshot_name,
            scope_desc
        );
        print!("Are you sure you want to continue? (y/N) ");
        use std::io::{self, Write};
        io::stdout()
            .flush()
            .map_err(|e| VmError::general(e, "Failed to flush stdout"))?;
        let mut response = String::new();
        io::stdin()
            .read_line(&mut response)
            .map_err(|e| VmError::general(e, "Failed to read user input"))?;
        if response.trim().to_lowercase() != "y" {
            vm_core::vm_println!("Snapshot deletion cancelled.");
            return Ok(());
        }
    }

    manager.delete_snapshot(scope, snapshot_name)?;
    vm_core::vm_success!("Snapshot '{}' deleted successfully", snapshot_name);

    Ok(())
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
}
