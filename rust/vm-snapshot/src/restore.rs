//! Snapshot restoration functionality

use crate::archive::validate_snapshot_files;
use crate::compose_plan::ComposeCapturePlan;
use crate::docker::{execute_docker_compose, execute_docker_compose_status, ComposeProject};
use crate::images::load_service_images;
use crate::manager::{snapshot_file_path, SnapshotManager, SnapshotScope};
use crate::metadata::SnapshotMetadata;
use crate::volumes::restore_volumes;
use std::path::Path;
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
    target_environment: Option<&str>,
    project_dir_override: Option<&Path>,
    force: bool,
) -> Result<()> {
    let manager = SnapshotManager::new()?;

    let project_name = project_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_project_name(config));
    let (_, snapshot_name) = SnapshotScope::from_name(name, Some(project_name.as_str()));
    let owner = config.vm.owning_config_path().ok_or_else(|| {
        VmError::validation(
            "Snapshot restore requires a project configuration",
            None::<String>,
        )
    })?;
    let scope = SnapshotScope::OwnedProject {
        name: &project_name,
        config_path: owner,
    };

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
    if metadata.owner_config_path.as_deref()
        != Some(owner.canonicalize()?.to_string_lossy().as_ref())
    {
        return Err(VmError::validation(
            "Snapshot has a different project configuration owner",
            None::<String>,
        ));
    }
    validate_snapshot_files(&snapshot_dir, &metadata)?;
    if target_environment.is_some() && metadata.source_environment.as_deref() != target_environment
    {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' belongs to environment '{}'",
                snapshot_name,
                metadata.source_environment.as_deref().unwrap_or("none")
            ),
            Some("Select the captured environment for restore"),
        ));
    }
    if metadata.provider != executable {
        return Err(VmError::validation(
            format!("Snapshot requires provider '{}'", metadata.provider),
            None::<String>,
        ));
    }
    if metadata.services.is_empty() {
        return Err(VmError::validation(
            "Container snapshot has no captured service images and cannot restore root filesystems",
            Some("Create a new snapshot with captured container services"),
        ));
    }
    if metadata.compose_file.is_empty()
        || !snapshot_file_path(
            &snapshot_dir.join("compose"),
            &metadata.compose_file,
            "compose file",
        )?
        .is_file()
    {
        return Err(VmError::validation(
            "Snapshot has no Compose configuration and cannot be restored by this provider",
            None::<String>,
        ));
    }

    // Verify project matches (skip for global snapshots)
    if !matches!(scope, SnapshotScope::Global) && metadata.project_name != project_name && !force {
        return Err(VmError::validation(
            format!(
                "Snapshot was created for project '{}' but current project is '{}'. Select the snapshot owner project.",
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
    let project_dir = match project_dir_override {
        Some(path) => path.to_path_buf(),
        None => {
            std::env::current_dir().map_err(|e| VmError::filesystem(e, "current_dir", "get"))?
        }
    };

    let compose =
        ComposeProject::for_environment(executable, target_environment, &project_dir).await?;
    let normalized =
        execute_docker_compose(executable, &["config", "--format", "json"], &compose).await?;
    let current_plan =
        ComposeCapturePlan::parse(&normalized, &owner.canonicalize()?.to_string_lossy())?;
    for volume in &metadata.volumes {
        if !current_plan.volumes.iter().any(|current| {
            current.name == volume.name && current.runtime_name == volume.runtime_name
        }) {
            return Err(VmError::validation(
                format!(
                    "Captured volume '{}' does not match the selected Compose project",
                    volume.name
                ),
                Some("Restore into the project with matching named volumes"),
            ));
        }
    }

    crate::volumes::verify_owned_volumes(executable, &current_plan.volumes).await?;

    // Load images
    tracing::info!("Loading service images in parallel...");
    let images_dir = snapshot_dir.join("images");
    load_service_images(executable, &images_dir, &metadata.services).await?;

    // Stop current compose environment
    tracing::info!("Stopping current environment...");
    execute_docker_compose_status(executable, &["down"], &compose).await?;

    // Restore volumes
    if !metadata.volumes.is_empty() {
        tracing::info!("Restoring volumes in parallel...");
        let volumes_dir = snapshot_dir.join("volumes");

        restore_volumes(executable, &volumes_dir, &metadata.volumes).await?;
    }

    // Restore configuration files
    tracing::info!("Restoring configuration files...");
    let compose_dir = snapshot_dir.join("compose");

    // Backup current files
    for config_file in &[&metadata.compose_file, &metadata.vm_config_file] {
        let source = snapshot_file_path(&compose_dir, config_file, "configuration file")?;
        let dest = if config_file.as_str() == metadata.compose_file {
            compose.file.clone()
        } else {
            snapshot_file_path(&project_dir, config_file, "configuration file")?
        };

        if source.exists() {
            // Create backup of existing file
            if dest.exists() {
                let backup_path = dest.with_extension("bak");
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
    execute_docker_compose_status(
        executable,
        &["up", "-d", "--no-build", "--pull", "never"],
        &compose,
    )
    .await?;

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
