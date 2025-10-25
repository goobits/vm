//! Snapshot restoration functionality

use super::manager::SnapshotManager;
use super::metadata::SnapshotMetadata;
use crate::error::{VmError, VmResult};
use std::path::Path;
use vm_config::AppConfig;

/// Execute docker compose command
async fn execute_docker_compose(args: &[&str], project_dir: &Path) -> VmResult<()> {
    let status = tokio::process::Command::new("docker")
        .arg("compose")
        .args(args)
        .current_dir(project_dir)
        .status()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker compose command"))?;

    if !status.success() {
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker compose command failed"),
            format!("Command 'docker compose {}' failed", args.join(" ")),
        ));
    }

    Ok(())
}

/// Execute docker command
async fn execute_docker(args: &[&str]) -> VmResult<()> {
    let status = tokio::process::Command::new("docker")
        .args(args)
        .status()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker command"))?;

    if !status.success() {
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker command failed"),
            format!("Command 'docker {}' failed", args.join(" ")),
        ));
    }

    Ok(())
}

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
) -> VmResult<()> {
    let manager = SnapshotManager::new()?;
    let project_name = project_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_project_name(config));

    // Load snapshot metadata
    let snapshot_dir = manager.get_snapshot_dir(Some(&project_name), name);
    let metadata_file = snapshot_dir.join("metadata.json");

    if !metadata_file.exists() {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' not found for project '{}'",
                name, project_name
            ),
            None::<String>,
        ));
    }

    let metadata = SnapshotMetadata::load(&metadata_file)?;

    // Verify project matches
    if metadata.project_name != project_name && !force {
        return Err(VmError::validation(
            format!(
                "Snapshot was created for project '{}' but current project is '{}'. Use --force to override.",
                metadata.project_name, project_name
            ),
            None::<String>,
        ));
    }

    vm_core::vm_println!(
        "Restoring snapshot '{}' for project '{}'...",
        name,
        project_name
    );

    // Get project directory
    let project_dir =
        std::env::current_dir().map_err(|e| VmError::filesystem(e, "current_dir", "get"))?;

    // Stop current compose environment
    vm_core::vm_println!("Stopping current environment...");
    execute_docker_compose(&["down"], &project_dir).await?;

    // Restore volumes
    if !metadata.volumes.is_empty() {
        vm_core::vm_println!("Restoring volumes...");
        let volumes_dir = snapshot_dir.join("volumes");

        for volume in &metadata.volumes {
            vm_core::vm_println!("  Restoring volume: {}", volume.name);

            let full_volume_name = format!("{}_{}", project_name, volume.name);

            // Remove existing volume if force is set
            if force {
                // Ignore errors if volume doesn't exist
                let _ = execute_docker(&["volume", "rm", &full_volume_name]).await;
            }

            // Create volume
            execute_docker(&["volume", "create", &full_volume_name]).await?;

            // Restore volume data
            let run_args = [
                "run",
                "--rm",
                "-v",
                &format!("{}:/data", full_volume_name),
                "-v",
                &format!("{}:/backup", volumes_dir.to_string_lossy()),
                "busybox",
                "tar",
                "xzf",
                &format!("/backup/{}", volume.archive_file),
                "-C",
                "/data",
            ];

            execute_docker(&run_args).await?;
        }
    }

    // Load images
    if !metadata.services.is_empty() {
        vm_core::vm_println!("Loading service images...");
        let images_dir = snapshot_dir.join("images");

        for service in &metadata.services {
            vm_core::vm_println!("  Loading image: {}", service.name);

            let image_path = images_dir.join(&service.image_file);
            execute_docker(&["load", "-i", image_path.to_str().unwrap()]).await?;
        }
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
                std::fs::copy(&dest, &backup_path)
                    .map_err(|e| VmError::filesystem(e, dest.to_string_lossy(), "copy"))?;
                vm_core::vm_println!("  Backed up {} to {}.bak", config_file, config_file);
            }

            // Restore from snapshot
            std::fs::copy(&source, &dest)
                .map_err(|e| VmError::filesystem(e, dest.to_string_lossy(), "copy"))?;
            vm_core::vm_println!("  Restored {}", config_file);
        }
    }

    // Start compose environment
    vm_core::vm_println!("Starting restored environment...");
    execute_docker_compose(&["up", "-d"], &project_dir).await?;

    vm_core::vm_success!("Snapshot '{}' restored successfully", name);

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
