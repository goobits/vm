//! Snapshot creation functionality

use crate::archive::directory_size;
use crate::base_image::create_from_dockerfile;
use crate::docker::{execute_docker_compose, execute_docker_with_output};
use crate::images::snapshot_container;
use crate::manager::{SnapshotManager, SnapshotScope};
use crate::metadata::{ServiceSnapshot, SnapshotMetadata};
use crate::optimal_concurrency;
use crate::volumes::backup_volumes;
use chrono::Utc;
use futures_util::stream::{self, StreamExt};
use std::path::Path;
use vm_config::AppConfig;
use vm_core::error::{Result, VmError};

/// Get git repository information
/// Optimized to use a single git command instead of 3 separate spawns (3x faster)
async fn get_git_info(project_dir: &Path) -> Result<(Option<String>, bool, Option<String>)> {
    // Use single git status command to get all info at once
    let output = tokio::process::Command::new("git")
        .args(["status", "--porcelain=v2", "--branch"])
        .current_dir(project_dir)
        .output()
        .await;

    let Ok(output) = output else {
        return Ok((None, false, None));
    };

    if !output.status.success() {
        return Ok((None, false, None));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut commit = None;
    let mut branch = None;
    let mut is_dirty = false;

    for line in stdout.lines() {
        if line.starts_with("# branch.oid ") {
            // Safe: we just checked that the line starts with this prefix
            if let Some(oid) = line.strip_prefix("# branch.oid ") {
                commit = Some(oid.to_string());
            }
        } else if line.starts_with("# branch.head ") {
            // Safe: we just checked that the line starts with this prefix
            if let Some(branch_name) = line.strip_prefix("# branch.head ") {
                if branch_name != "(detached)" {
                    branch = Some(branch_name.to_string());
                }
            }
        } else if !line.starts_with('#') && !line.is_empty() {
            // Any non-header, non-empty line indicates changes
            is_dirty = true;
        }
    }

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
#[allow(clippy::too_many_arguments)]
pub async fn handle_create(
    config: &AppConfig,
    executable: &str,
    name: &str,
    description: Option<&str>,
    quiesce: bool,
    project_override: Option<&str>,
    from_dockerfile: Option<&std::path::Path>,
    build_context: Option<&std::path::Path>,
    build_args: &[String],
    force: bool,
) -> Result<()> {
    let manager = SnapshotManager::new()?;

    // Handle --from-dockerfile mode
    if let Some(dockerfile_path) = from_dockerfile {
        let ctx = build_context.unwrap_or_else(|| std::path::Path::new("."));
        return create_from_dockerfile(
            executable,
            name,
            description,
            dockerfile_path,
            ctx,
            build_args,
            force,
        )
        .await;
    }

    // Determine if this is a global snapshot (@name) or project-specific (name)
    let project_name = project_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| get_project_name(config));
    let (scope, snapshot_name) = SnapshotScope::from_name(name, Some(project_name.as_str()));

    // Check if snapshot already exists
    if manager.snapshot_exists(scope, snapshot_name)? && !force {
        let scope_desc = if matches!(scope, SnapshotScope::Global) {
            "global".to_string()
        } else {
            format!("project '{}'", project_name)
        };
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' already exists for {}. Use --force to overwrite.",
                snapshot_name, scope_desc
            ),
            None::<String>,
        ));
    }

    let display_scope = if matches!(scope, SnapshotScope::Global) {
        "globally".to_string()
    } else {
        format!("for project '{}'", project_name)
    };

    tracing::info!("Creating snapshot '{}' {}...", snapshot_name, display_scope);

    // Get project directory
    let project_dir =
        std::env::current_dir().map_err(|e| VmError::filesystem(e, "current_dir", "get"))?;

    // Create snapshot directory structure
    let staging = manager.create_staging_dir(scope, snapshot_name)?;
    let snapshot_dir = staging.path().to_path_buf();
    let images_dir = snapshot_dir.join("images");
    let volumes_dir = snapshot_dir.join("volumes");
    let compose_dir = snapshot_dir.join("compose");

    for dir in [&snapshot_dir, &images_dir, &volumes_dir, &compose_dir] {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| VmError::filesystem(e, dir.to_string_lossy(), "create_dir_all"))?;
    }

    let has_compose_file = [
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
    ]
    .iter()
    .any(|file| project_dir.join(file).exists());

    let (services, volumes) = if has_compose_file {
        // Quiesce containers if requested
        if quiesce {
            tracing::info!("Pausing containers for consistent snapshot...");
            if let Err(error) = execute_docker_compose(executable, &["pause"], &project_dir).await {
                let resume = execute_docker_compose(executable, &["unpause"], &project_dir)
                    .await
                    .map(|_| ());
                return finish_quiesce(Err(error), resume);
            }
        }

        let snapshot_result = snapshot_compose_services(
            executable,
            &project_name,
            snapshot_name,
            &project_dir,
            &images_dir,
        )
        .await;

        let resume_result = if quiesce {
            tracing::info!("Unpausing containers...");
            execute_docker_compose(executable, &["unpause"], &project_dir)
                .await
                .map(|_| ())
        } else {
            Ok(())
        };
        let services = finish_quiesce(snapshot_result, resume_result)?;

        // Discover volumes
        tracing::info!("Discovering volumes...");
        let volumes_output =
            execute_docker_compose(executable, &["config", "--volumes"], &project_dir).await?;
        let volume_names: Vec<String> = volumes_output
            .lines()
            .filter(|line| !line.is_empty())
            .map(|s| s.to_string())
            .collect();

        tracing::info!("Backing up volumes in parallel...");
        let volumes =
            backup_volumes(executable, &project_name, &volumes_dir, &volume_names).await?;

        (services, volumes)
    } else {
        tracing::info!("Discovering VM-managed containers...");
        let project_filter = format!("label=com.vm.project={project_name}");
        let containers_output = execute_docker_with_output(
            executable,
            &[
                "ps",
                "--filter",
                &project_filter,
                "--format",
                "{{.ID}}\t{{.Names}}",
            ],
        )
        .await?;
        let containers: Vec<(String, String)> = containers_output
            .lines()
            .filter_map(|line| {
                let (id, name) = line.split_once('\t')?;
                Some((id.to_string(), name.to_string()))
            })
            .collect();

        if containers.is_empty() {
            return Err(VmError::validation(
                format!(
                    "No running VM containers found for project '{}'",
                    project_name
                ),
                Some("Start the VM first, then run `vm snapshot create <name>`.".to_string()),
            ));
        }

        let paused_containers = if quiesce {
            tracing::info!("Pausing containers for consistent snapshot...");
            pause_containers(executable, &containers).await?
        } else {
            Vec::new()
        };

        let snapshot_result = snapshot_containers(
            executable,
            &project_name,
            snapshot_name,
            &containers,
            &images_dir,
        )
        .await;

        let resume_result = if quiesce {
            tracing::info!("Unpausing containers...");
            resume_containers(executable, &paused_containers).await
        } else {
            Ok(())
        };
        let services = finish_quiesce(snapshot_result, resume_result)?;

        (services, Vec::new())
    };

    // Copy configuration files
    tracing::info!("Copying configuration files...");
    let compose_file = "docker-compose.yml";
    let vm_config_file = "vm.yaml";

    if project_dir.join(compose_file).exists() {
        tokio::fs::copy(
            project_dir.join(compose_file),
            compose_dir.join(compose_file),
        )
        .await
        .map_err(|e| VmError::filesystem(e, compose_file, "copy"))?;
    }

    if project_dir.join(vm_config_file).exists() {
        tokio::fs::copy(
            project_dir.join(vm_config_file),
            compose_dir.join(vm_config_file),
        )
        .await
        .map_err(|e| VmError::filesystem(e, vm_config_file, "copy"))?;
    }

    // Get git information
    let (git_commit, git_dirty, git_branch) = get_git_info(&project_dir).await?;

    // Calculate total size
    let total_size_bytes = directory_size(&snapshot_dir)?;

    // Build and save metadata
    let metadata = SnapshotMetadata {
        name: snapshot_name.to_string(),
        created_at: Utc::now(),
        description: description.map(|s| s.to_string()),
        project_name: scope.project_name().to_string(),
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
    manager.install_staged_snapshot(staging, scope, snapshot_name, force)?;

    tracing::info!(
        "Snapshot '{}' created successfully ({:.2} MB)",
        name,
        total_size_bytes as f64 / (1024.0 * 1024.0)
    );

    Ok(())
}

async fn snapshot_compose_services(
    executable: &str,
    project_name: &str,
    snapshot_name: &str,
    project_dir: &Path,
    images_dir: &Path,
) -> Result<Vec<ServiceSnapshot>> {
    tracing::info!("Discovering services...");
    let services_output =
        execute_docker_compose(executable, &["ps", "--services"], project_dir).await?;
    let service_names: Vec<String> = services_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    tracing::info!("Snapshotting services in parallel...");
    let futures = service_names.iter().map(|service| {
        let service = service.clone();
        let project_name = project_name.to_string();
        let snapshot_name = snapshot_name.to_string();
        let images_dir = images_dir.to_path_buf();
        let project_dir = project_dir.to_path_buf();
        async move {
            let container_id =
                execute_docker_compose(executable, &["ps", "-q", &service], &project_dir).await?;
            if container_id.is_empty() {
                tracing::warn!("Service '{}' has no running container, skipping", service);
                return Ok::<Option<ServiceSnapshot>, VmError>(None);
            }
            snapshot_container(
                executable,
                &project_name,
                &snapshot_name,
                &service,
                &container_id,
                &images_dir,
            )
            .await
            .map(Some)
        }
    });
    stream::iter(futures)
        .buffer_unordered(optimal_concurrency())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()
        .map(|services| services.into_iter().flatten().collect())
}

async fn snapshot_containers(
    executable: &str,
    project_name: &str,
    snapshot_name: &str,
    containers: &[(String, String)],
    images_dir: &Path,
) -> Result<Vec<ServiceSnapshot>> {
    tracing::info!("Snapshotting containers in parallel...");
    let futures = containers.iter().map(|(container_id, container_name)| {
        let project_name = project_name.to_string();
        let snapshot_name = snapshot_name.to_string();
        let images_dir = images_dir.to_path_buf();
        let service_name = container_name.clone();
        let container_id = container_id.clone();
        async move {
            snapshot_container(
                executable,
                &project_name,
                &snapshot_name,
                &service_name,
                &container_id,
                &images_dir,
            )
            .await
        }
    });
    stream::iter(futures)
        .buffer_unordered(optimal_concurrency())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect()
}

async fn pause_containers(
    executable: &str,
    containers: &[(String, String)],
) -> Result<Vec<String>> {
    let mut paused = Vec::new();
    for (container_id, _) in containers {
        match execute_docker_with_output(executable, &["pause", container_id]).await {
            Ok(_) => paused.push(container_id.clone()),
            Err(error) => {
                return finish_quiesce(Err(error), resume_containers(executable, &paused).await);
            }
        }
    }
    Ok(paused)
}

async fn resume_containers(executable: &str, containers: &[String]) -> Result<()> {
    let mut result = Ok(());
    for container_id in containers {
        if let Err(error) = execute_docker_with_output(executable, &["unpause", container_id]).await
        {
            result = Err(error);
        }
    }
    result
}

fn finish_quiesce<T>(snapshot: Result<T>, resume: Result<()>) -> Result<T> {
    match (snapshot, resume) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(snapshot_error), Err(resume_error)) => Err(VmError::general(
            resume_error,
            format!(
                "Snapshot failed ({snapshot_error}) and paused containers could not be resumed"
            ),
        )),
    }
}
