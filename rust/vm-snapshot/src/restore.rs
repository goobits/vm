//! Snapshot restoration functionality

use crate::archive::validate_snapshot_files;
use crate::docker::execute_docker_compose_status;
use crate::images::load_service_images;
use crate::manager::{snapshot_file_path, SnapshotManager, SnapshotScope};
use crate::metadata::SnapshotMetadata;
use crate::volumes::restore_volumes;
use vm_config::AppConfig;
use vm_core::error::{Result, VmError};

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
    executable: &str,
    name: &str,
    project_override: Option<&str>,
    force: bool,
) -> Result<()> {
    let manager = SnapshotManager::new()?;

    let project_name = project_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_project_name(config));
    let (scope, snapshot_name) = SnapshotScope::from_name(name, Some(project_name.as_str()));

    // Load snapshot metadata
    let snapshot_dir = manager.get_snapshot_dir(scope, snapshot_name)?;
    let metadata_file = snapshot_dir.join("metadata.json");

    if !metadata_file.is_file() {
        let scope_desc = if matches!(scope, SnapshotScope::Global) {
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
    validate_snapshot_files(&snapshot_dir, &metadata)?;

    // Verify project matches (skip for global snapshots)
    if !matches!(scope, SnapshotScope::Global) && metadata.project_name != project_name && !force {
        return Err(VmError::validation(
            format!(
                "Snapshot was created for project '{}' but current project is '{}'. Use --force to override.",
                metadata.project_name, project_name
            ),
            None::<String>,
        ));
    }

    let scope_desc = if matches!(scope, SnapshotScope::Global) {
        "globally".to_string()
    } else {
        format!("for project '{}'", project_name)
    };
    tracing::info!("Restoring snapshot '{}' {}...", snapshot_name, scope_desc);

    // Get project directory
    let project_dir =
        std::env::current_dir().map_err(|e| VmError::filesystem(e, "current_dir", "get"))?;

    // Stop current compose environment
    tracing::info!("Stopping current environment...");
    execute_docker_compose_status(executable, &["down"], &project_dir).await?;

    // Restore volumes
    if !metadata.volumes.is_empty() {
        tracing::info!("Restoring volumes in parallel...");
        let volumes_dir = snapshot_dir.join("volumes");

        restore_volumes(
            executable,
            &project_name,
            &volumes_dir,
            &metadata.volumes,
            force,
        )
        .await?;
    }

    // Load images
    if !metadata.services.is_empty() {
        tracing::info!("Loading service images in parallel...");
        let images_dir = snapshot_dir.join("images");

        load_service_images(executable, &images_dir, &metadata.services).await?;
    }

    // Restore configuration files
    tracing::info!("Restoring configuration files...");
    let compose_dir = snapshot_dir.join("compose");

    // Backup current files
    for config_file in &[&metadata.compose_file, &metadata.vm_config_file] {
        let source = snapshot_file_path(&compose_dir, config_file, "configuration file")?;
        let dest = snapshot_file_path(&project_dir, config_file, "configuration file")?;

        if source.exists() {
            // Create backup of existing file
            if dest.exists() {
                let backup_name = format!("{config_file}.bak");
                let backup_path =
                    snapshot_file_path(&project_dir, &backup_name, "configuration backup")?;
                tokio::fs::copy(&dest, &backup_path)
                    .await
                    .map_err(|e| VmError::filesystem(e, dest.to_string_lossy(), "copy"))?;
                tracing::info!("  Backed up {} to {}.bak", config_file, config_file);
            }

            // Restore from snapshot
            tokio::fs::copy(&source, &dest)
                .await
                .map_err(|e| VmError::filesystem(e, dest.to_string_lossy(), "copy"))?;
            tracing::info!("  Restored {}", config_file);
        }
    }

    // Start compose environment
    tracing::info!("Starting restored environment...");
    execute_docker_compose_status(executable, &["up", "-d"], &project_dir).await?;

    tracing::info!("Snapshot '{}' restored successfully", snapshot_name);

    // Show git info if available
    if let Some(branch) = &metadata.git_branch {
        let dirty = if metadata.git_dirty {
            " (was dirty)"
        } else {
            ""
        };
        tracing::info!(
            "\nSnapshot was created from git branch '{}' @ {}{}",
            branch,
            metadata.git_commit.as_deref().unwrap_or("unknown"),
            dirty
        );
    }

    Ok(())
}
