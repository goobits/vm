//! Snapshot restoration functionality

use crate::docker::{execute_docker, execute_docker_compose_status, execute_docker_streaming};
use crate::manager::SnapshotManager;
use crate::metadata::SnapshotMetadata;
use crate::optimal_concurrency;
use vm_core::error::{VmError, Result};
use futures::stream::{self, StreamExt};
use vm_config::AppConfig;

/// Get project name from config
fn get_project_name(config: &AppConfig) -> String {
    config
        .vm
        .project
        .as_ref()
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| "default".to_string())
}

/// Handle snapshot restoration
pub async fn handle_restore(
    config: &AppConfig,
    name: &str,
    project_override: Option<&str>,
    force: bool,
) -> Result<()> {
    let manager = SnapshotManager::new()?;

    // Determine if this is a global snapshot (@name) or project-specific (name)
    let is_global = name.starts_with('@');
    let snapshot_name = if is_global {
        name.trim_start_matches('@')
    } else {
        name
    };

    let project_name = project_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_project_name(config));

    // For global snapshots, use None as project scope
    let project_scope = if is_global {
        None
    } else {
        Some(project_name.as_str())
    };

    // Load snapshot metadata
    let snapshot_dir = manager.get_snapshot_dir(project_scope, snapshot_name);
    let metadata_file = snapshot_dir.join("metadata.json");

    if !metadata_file.exists() {
        let scope_desc = if is_global {
            "global snapshots".to_string()
        } else {
            format!("project '{}'", project_name)
        };
        return Err(VmError::validation(
            format!("Snapshot '{}' not found in {}", snapshot_name, scope_desc),
            None::<String>,
        ));
    }

    let metadata = SnapshotMetadata::load(&metadata_file)?;

    // Verify project matches (skip for global snapshots)
    if !is_global && metadata.project_name != project_name && !force {
        return Err(VmError::validation(
            format!(
                "Snapshot was created for project '{}' but current project is '{}'. Use --force to override.",
                metadata.project_name, project_name
            ),
            None::<String>,
        ));
    }

    let scope_desc = if is_global {
        "globally".to_string()
    } else {
        format!("for project '{}'", project_name)
    };
    vm_core::vm_println!("Restoring snapshot '{}' {}...", snapshot_name, scope_desc);

    // Get project directory
    let project_dir =
        std::env::current_dir().map_err(|e| VmError::filesystem(e, "current_dir", "get"))?;

    // Stop current compose environment
    vm_core::vm_println!("Stopping current environment...");
    execute_docker_compose_status(&["down"], &project_dir).await?;

    // Restore volumes
    if !metadata.volumes.is_empty() {
        vm_core::vm_println!("Restoring volumes in parallel...");
        let volumes_dir = snapshot_dir.join("volumes");

        // Parallelize volume restoration for 2-4x faster restore
        let volume_futures = metadata.volumes.iter().map(|volume| {
            let volume_name = volume.name.clone();
            let archive_file = volume.archive_file.clone();
            let project_name = project_name.clone();
            let volumes_dir = volumes_dir.clone();

            async move {
                vm_core::vm_println!("  Restoring volume: {}", volume_name);

                let full_volume_name = format!("{}_{}", project_name, volume_name);

                // Remove existing volume if force is set
                if force {
                    // Try to remove volume, but don't fail if it doesn't exist or is in use
                    // If in use, we'll try to restore anyway (will overwrite contents)
                    let _ = execute_docker(&["volume", "rm", &full_volume_name]).await;
                }

                // Create volume - ignore "already exists" error since we'll restore over it
                // This handles both the case where rm failed (volume in use) and force=false
                let _ = execute_docker(&["volume", "create", &full_volume_name]).await;

                // Restore volume data with zstd decompression (3-5x faster than gzip)
                // Support both .tar.zst (new) and .tar.gz (legacy) formats
                let restore_cmd = if archive_file.ends_with(".tar.zst") {
                    format!("zstd -d -c /backup/{} | tar -x -C /data", archive_file)
                } else {
                    // Legacy .tar.gz format
                    format!("tar -xzf /backup/{} -C /data", archive_file)
                };

                let run_args = [
                    "run",
                    "--rm",
                    "-v",
                    &format!("{}:/data", full_volume_name),
                    "-v",
                    &format!("{}:/backup", volumes_dir.to_string_lossy()),
                    "alpine:latest",
                    "sh",
                    "-c",
                    &restore_cmd,
                ];

                // Stream output so users see decompression progress
                execute_docker_streaming(&run_args).await?;
                Ok::<_, VmError>(())
            }
        });

        // Restore volumes concurrently (CPU-adaptive concurrency)
        stream::iter(volume_futures)
            .buffer_unordered(optimal_concurrency())
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
    }

    // Load images
    if !metadata.services.is_empty() {
        vm_core::vm_println!("Loading service images in parallel...");
        let images_dir = snapshot_dir.join("images");

        // Parallelize image loading for 2-5x faster restoration
        let load_futures = metadata.services.iter().map(|service| {
            let service_name = service.name.clone();
            let image_file = service.image_file.clone();
            let images_dir = images_dir.clone();

            async move {
                vm_core::vm_println!("  Loading image: {}", service_name);

                let image_path = images_dir.join(&image_file);
                let image_path_str = image_path.to_str().ok_or_else(|| {
                    VmError::general(
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Invalid UTF-8 in path",
                        ),
                        format!(
                            "Snapshot path contains invalid UTF-8 characters: {}",
                            image_path.display()
                        ),
                    )
                })?;
                // Stream output so users see "Loaded image: ..." progress
                execute_docker_streaming(&["load", "-i", image_path_str]).await?;
                Ok::<_, VmError>(())
            }
        });

        // Load images concurrently (CPU-adaptive concurrency)
        stream::iter(load_futures)
            .buffer_unordered(optimal_concurrency())
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
    }

    // Restore configuration files
    vm_core::vm_println!("Restoring configuration files...");
    let compose_dir = snapshot_dir.join("compose");

    // Backup current files
    for config_file in &[&metadata.compose_file, &metadata.vm_config_file] {
        let source = compose_dir.join(config_file);
        let dest = project_dir.join(config_file);

        if source.exists() {
            // Create backup of existing file
            if dest.exists() {
                let backup_path = project_dir.join(format!("{}.bak", config_file));
                tokio::fs::copy(&dest, &backup_path)
                    .await
                    .map_err(|e| VmError::filesystem(e, dest.to_string_lossy(), "copy"))?;
                vm_core::vm_println!("  Backed up {} to {}.bak", config_file, config_file);
            }

            // Restore from snapshot
            tokio::fs::copy(&source, &dest)
                .await
                .map_err(|e| VmError::filesystem(e, dest.to_string_lossy(), "copy"))?;
            vm_core::vm_println!("  Restored {}", config_file);
        }
    }

    // Start compose environment
    vm_core::vm_println!("Starting restored environment...");
    execute_docker_compose_status(&["up", "-d"], &project_dir).await?;

    vm_core::vm_success!("Snapshot '{}' restored successfully", snapshot_name);

    // Show git info if available
    if let Some(branch) = &metadata.git_branch {
        let dirty = if metadata.git_dirty {
            " (was dirty)"
        } else {
            ""
        };
        vm_core::vm_println!(
            "\nSnapshot was created from git branch '{}' @ {}{}",
            branch,
            metadata.git_commit.as_deref().unwrap_or("unknown"),
            dirty
        );
    }

    Ok(())
}
