//! Snapshot creation functionality

use super::manager::SnapshotManager;
use super::metadata::{ServiceSnapshot, SnapshotMetadata, VolumeSnapshot};
use crate::error::{VmError, VmResult};
use chrono::Utc;
use vm_config::AppConfig;

/// Execute docker compose command and return output
async fn execute_docker_compose(args: &[&str], project_dir: &std::path::Path) -> VmResult<String> {
    let output = tokio::process::Command::new("docker")
        .arg("compose")
        .args(args)
        .current_dir(project_dir)
        .output()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker compose command"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker compose command failed"),
            format!("Command failed: {}", stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute docker command and return output
async fn execute_docker(args: &[&str]) -> VmResult<String> {
    let output = tokio::process::Command::new("docker")
        .args(args)
        .output()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker command"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker command failed"),
            format!("Command failed: {}", stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get git repository information
async fn get_git_info(
    project_dir: &std::path::Path,
) -> VmResult<(Option<String>, bool, Option<String>)> {
    // Get commit hash
    let commit = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_dir)
        .output()
        .await
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        });

    // Check if working directory is dirty
    let is_dirty = tokio::process::Command::new("git")
        .args(["diff-index", "--quiet", "HEAD"])
        .current_dir(project_dir)
        .status()
        .await
        .map(|status| !status.success())
        .unwrap_or(false);

    // Get current branch
    let branch = tokio::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(project_dir)
        .output()
        .await
        .ok()
        .and_then(|output| {
            if output.status.success() {
                Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
            } else {
                None
            }
        });

    Ok((commit, is_dirty, branch))
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

/// Handle snapshot creation
pub async fn handle_create(
    config: &AppConfig,
    name: &str,
    description: Option<&str>,
    quiesce: bool,
    project_override: Option<&str>,
) -> VmResult<()> {
    let manager = SnapshotManager::new()?;
    let project_name = project_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_project_name(config));

    // Check if snapshot already exists
    if manager.snapshot_exists(Some(&project_name), name) {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' already exists for project '{}'",
                name, project_name
            ),
            None::<String>,
        ));
    }

    vm_core::vm_println!(
        "Creating snapshot '{}' for project '{}'...",
        name,
        project_name
    );

    // Get project directory
    let project_dir =
        std::env::current_dir().map_err(|e| VmError::filesystem(e, "current_dir", "get"))?;

    // Create snapshot directory structure
    let snapshot_dir = manager.get_snapshot_dir(Some(&project_name), name);
    let images_dir = snapshot_dir.join("images");
    let volumes_dir = snapshot_dir.join("volumes");
    let compose_dir = snapshot_dir.join("compose");

    for dir in [&snapshot_dir, &images_dir, &volumes_dir, &compose_dir] {
        std::fs::create_dir_all(dir)
            .map_err(|e| VmError::filesystem(e, dir.to_string_lossy(), "create_dir_all"))?;
    }

    // Quiesce containers if requested
    if quiesce {
        vm_core::vm_println!("Pausing containers for consistent snapshot...");
        execute_docker_compose(&["pause"], &project_dir).await?;
    }

    // Discover services
    vm_core::vm_println!("Discovering services...");
    let services_output = execute_docker_compose(&["ps", "--services"], &project_dir).await?;
    let service_names: Vec<String> = services_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut services = Vec::new();

    // Snapshot each service
    for service in &service_names {
        vm_core::vm_println!("  Snapshotting service: {}", service);

        // Get container ID for this service
        let container_id = execute_docker_compose(&["ps", "-q", service], &project_dir).await?;
        if container_id.is_empty() {
            vm_core::vm_warning!("Service '{}' has no running container, skipping", service);
            continue;
        }

        // Create image from container
        let image_tag = format!("vm-snapshot/{}/{}:{}", project_name, service, name);
        execute_docker(&["commit", &container_id, &image_tag]).await?;

        // Save image to tar file
        let image_file = format!("{}.tar", service);
        let image_path = images_dir.join(&image_file);
        execute_docker(&["save", &image_tag, "-o", image_path.to_str().unwrap()]).await?;

        // Get image digest
        let digest_output = execute_docker(&["inspect", "--format={{.Id}}", &image_tag]).await?;
        let digest = if digest_output.is_empty() {
            None
        } else {
            Some(digest_output)
        };

        services.push(ServiceSnapshot {
            name: service.clone(),
            image_tag,
            image_file,
            image_digest: digest,
        });
    }

    // Unpause containers if we paused them
    if quiesce {
        vm_core::vm_println!("Unpausing containers...");
        execute_docker_compose(&["unpause"], &project_dir).await?;
    }

    // Discover volumes
    vm_core::vm_println!("Discovering volumes...");
    let volumes_output = execute_docker_compose(&["config", "--volumes"], &project_dir).await?;
    let volume_names: Vec<String> = volumes_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut volumes = Vec::new();

    // Snapshot each volume
    for volume in &volume_names {
        vm_core::vm_println!("  Backing up volume: {}", volume);

        let archive_file = format!("{}.tar.gz", volume);
        let archive_path = volumes_dir.join(&archive_file);

        // Get full volume name (might be prefixed with project name)
        let full_volume_name = format!("{}_{}", project_name, volume);

        // Export volume data using a temporary container
        let run_args = [
            "run",
            "--rm",
            "-v",
            &format!("{}:/data", full_volume_name),
            "-v",
            &format!("{}:/backup", volumes_dir.to_string_lossy()),
            "busybox",
            "tar",
            "czf",
            &format!("/backup/{}", archive_file),
            "-C",
            "/data",
            ".",
        ];

        execute_docker(&run_args).await?;

        // Get archive size
        let metadata = std::fs::metadata(&archive_path)
            .map_err(|e| VmError::filesystem(e, archive_path.to_string_lossy(), "metadata"))?;

        volumes.push(VolumeSnapshot {
            name: volume.clone(),
            archive_file,
            size_bytes: metadata.len(),
        });
    }

    // Copy configuration files
    vm_core::vm_println!("Copying configuration files...");
    let compose_file = "docker-compose.yml";
    let vm_config_file = "vm.yaml";

    if project_dir.join(compose_file).exists() {
        std::fs::copy(
            project_dir.join(compose_file),
            compose_dir.join(compose_file),
        )
        .map_err(|e| VmError::filesystem(e, compose_file, "copy"))?;
    }

    if project_dir.join(vm_config_file).exists() {
        std::fs::copy(
            project_dir.join(vm_config_file),
            compose_dir.join(vm_config_file),
        )
        .map_err(|e| VmError::filesystem(e, vm_config_file, "copy"))?;
    }

    // Get git information
    let (git_commit, git_dirty, git_branch) = get_git_info(&project_dir).await?;

    // Calculate total size
    let total_size_bytes = calculate_directory_size(&snapshot_dir)?;

    // Build and save metadata
    let metadata = SnapshotMetadata {
        name: name.to_string(),
        created_at: Utc::now(),
        description: description.map(|s| s.to_string()),
        project_name: project_name.clone(),
        project_dir: project_dir.to_string_lossy().to_string(),
        git_commit,
        git_dirty,
        git_branch,
        services,
        volumes,
        compose_file: compose_file.to_string(),
        vm_config_file: vm_config_file.to_string(),
        total_size_bytes,
    };

    metadata.save(snapshot_dir.join("metadata.json"))?;

    vm_core::vm_success!(
        "Snapshot '{}' created successfully ({:.2} MB)",
        name,
        total_size_bytes as f64 / (1024.0 * 1024.0)
    );

    Ok(())
}

/// Calculate total size of directory recursively
fn calculate_directory_size(path: &std::path::Path) -> VmResult<u64> {
    let mut total = 0u64;

    if path.is_dir() {
        let entries = std::fs::read_dir(path)
            .map_err(|e| VmError::filesystem(e, path.to_string_lossy(), "read_dir"))?;

        for entry in entries {
            let entry = entry.map_err(|e| VmError::general(e, "Failed to read directory entry"))?;
            let path = entry.path();

            if path.is_dir() {
                total += calculate_directory_size(&path)?;
            } else {
                let metadata = std::fs::metadata(&path)
                    .map_err(|e| VmError::filesystem(e, path.to_string_lossy(), "metadata"))?;
                total += metadata.len();
            }
        }
    }

    Ok(total)
}
