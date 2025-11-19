//! Snapshot management and lifecycle operations

use super::metadata::SnapshotMetadata;
use crate::error::{VmError, VmResult};
use std::path::PathBuf;

/// Manages snapshot storage and lifecycle
pub struct SnapshotManager {
    snapshots_dir: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub fn new() -> VmResult<Self> {
        let snapshots_dir = vm_core::user_paths::user_config_dir()?.join("snapshots");

        // Create snapshots directory if it doesn't exist
        std::fs::create_dir_all(&snapshots_dir).map_err(|e| {
            VmError::filesystem(e, snapshots_dir.to_string_lossy(), "create_dir_all")
        })?;

        Ok(Self { snapshots_dir })
    }

    /// Get the directory path for a specific snapshot
    pub fn get_snapshot_dir(&self, project: Option<&str>, name: &str) -> PathBuf {
        match project {
            Some(proj) => self.snapshots_dir.join(proj).join(name),
            None => self.snapshots_dir.join("global").join(name),
        }
    }

    /// List all snapshots, optionally filtered by project
    pub fn list_snapshots(&self, project_filter: Option<&str>) -> VmResult<Vec<SnapshotMetadata>> {
        let mut snapshots = Vec::new();

        // Determine which directories to scan
        let scan_dirs: Vec<PathBuf> = if let Some(project) = project_filter {
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
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(snapshots)
    }

    /// Delete a snapshot
    pub fn delete_snapshot(&self, project: Option<&str>, name: &str) -> VmResult<()> {
        let snapshot_dir = self.get_snapshot_dir(project, name);

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
    pub fn snapshot_exists(&self, project: Option<&str>, name: &str) -> bool {
        let snapshot_dir = self.get_snapshot_dir(project, name);
        snapshot_dir.exists() && snapshot_dir.join("metadata.json").exists()
    }
}

/// Handle the list subcommand
pub async fn handle_list(project: Option<&str>) -> VmResult<()> {
    let manager = SnapshotManager::new()?;
    let snapshots = manager.list_snapshots(project)?;

    if snapshots.is_empty() {
        vm_core::vm_println!("No snapshots found.");
        return Ok(());
    }

    vm_core::vm_println!("\nAvailable snapshots:\n");

    for snapshot in snapshots {
        vm_core::vm_println!("  {} ({})", snapshot.name, snapshot.project_name);
        vm_core::vm_println!(
            "    Created: {}",
            snapshot.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );

        if let Some(desc) = &snapshot.description {
            vm_core::vm_println!("    Description: {}", desc);
        }

        if let Some(branch) = &snapshot.git_branch {
            let dirty = if snapshot.git_dirty { " (dirty)" } else { "" };
            vm_core::vm_println!(
                "    Git: {} @ {}{}",
                branch,
                snapshot.git_commit.as_deref().unwrap_or("unknown"),
                dirty
            );
        }

        vm_core::vm_println!("    Services: {}", snapshot.services.len());
        vm_core::vm_println!("    Volumes: {}", snapshot.volumes.len());

        let size_mb = snapshot.total_size_bytes as f64 / (1024.0 * 1024.0);
        vm_core::vm_println!("    Size: {:.2} MB", size_mb);
        vm_core::vm_println!();
    }

    Ok(())
}

/// Handle the delete subcommand
pub async fn handle_delete(name: &str, project: Option<&str>, force: bool) -> VmResult<()> {
    let manager = SnapshotManager::new()?;

    if !manager.snapshot_exists(project, name) {
        return Err(VmError::validation(
            format!("Snapshot '{}' not found", name),
            None::<String>,
        ));
    }

    if !force {
        vm_core::vm_println!("This will permanently delete the snapshot '{}'.", name);
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

    manager.delete_snapshot(project, name)?;
    vm_core::vm_success!("Snapshot '{}' deleted successfully", name);

    Ok(())
}
