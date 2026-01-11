//! Snapshot management and lifecycle operations

use crate::metadata::SnapshotMetadata;
use vm_core::error::{VmError, Result};
use std::path::PathBuf;

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
    pub fn get_snapshot_dir(&self, project: Option<&str>, name: &str) -> PathBuf {
        match project {
            Some(proj) => self.snapshots_dir.join(proj).join(name),
            None => self.snapshots_dir.join("global").join(name),
        }
    }

    /// List all snapshots, optionally filtered by project
    pub fn list_snapshots(&self, project_filter: Option<&str>) -> Result<Vec<SnapshotMetadata>> {
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
    pub fn delete_snapshot(&self, project: Option<&str>, name: &str) -> Result<()> {
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
pub async fn handle_list(project: Option<&str>, snapshot_type: Option<&str>) -> Result<()> {
    let manager = SnapshotManager::new()?;
    let mut snapshots = manager.list_snapshots(project)?;

    // Filter by type if specified
    if let Some(filter_type) = snapshot_type {
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
        vm_core::vm_println!("{}", vm_messages::messages::MESSAGES.vm.snapshot_list_empty);
        return Ok(());
    }

    vm_core::vm_println!(
        "\n{}\n",
        vm_messages::messages::MESSAGES
            .vm
            .snapshot_list_table_header
    );
    vm_core::vm_println!(
        "{}",
        vm_messages::messages::MESSAGES
            .vm
            .snapshot_list_table_separator
    );

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
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Handle the delete subcommand
pub async fn handle_delete(name: &str, project: Option<&str>, force: bool) -> Result<()> {
    let manager = SnapshotManager::new()?;

    // Determine if this is a global snapshot (@name) or project-specific (name)
    let is_global = name.starts_with('@');
    let snapshot_name = if is_global {
        name.trim_start_matches('@')
    } else {
        name
    };

    // For global snapshots, use None as project scope (ignore any --project flag)
    let project_scope = if is_global { None } else { project };

    if !manager.snapshot_exists(project_scope, snapshot_name) {
        let scope_desc = if is_global {
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
        let scope_desc = if is_global {
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

    manager.delete_snapshot(project_scope, snapshot_name)?;
    vm_core::vm_success!("Snapshot '{}' deleted successfully", snapshot_name);

    Ok(())
}
