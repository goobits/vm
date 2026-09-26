//! Snapshot creation functionality

use crate::archive::directory_size;
use crate::base_image::create_from_dockerfile;
use crate::compose_plan::ComposeCapturePlan;
use crate::docker::{execute_docker_compose, ComposeProject};
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
    source_environment: Option<&str>,
    project_dir_override: Option<&Path>,
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
    let (_, snapshot_name) = SnapshotScope::from_name(name, Some(project_name.as_str()));
    let owner = config.vm.owning_config_path().ok_or_else(|| {
        VmError::validation(
            "Snapshot creation requires a project configuration",
            None::<String>,
        )
    })?;
    let scope = SnapshotScope::OwnedProject {
        name: &project_name,
        config_path: owner,
    };

    // Check if snapshot already exists
    if manager.snapshot_exists(scope, snapshot_name)? && !force {
        let scope_desc = if matches!(scope, SnapshotScope::Global) {
            "global".to_string()
        } else {
            format!("project '{}'", project_name)
        };
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' already exists for {}. Choose a new snapshot name.",
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
    let project_dir = match project_dir_override {
        Some(path) => path.to_path_buf(),
        None => {
            std::env::current_dir().map_err(|e| VmError::filesystem(e, "current_dir", "get"))?
        }
    };

    let compose =
        ComposeProject::for_environment(executable, source_environment, &project_dir).await?;
    let compose_file = "compose.yaml";

    let normalized =
        execute_docker_compose(executable, &["config", "--format", "json"], &compose).await?;
    let capture_plan =
        ComposeCapturePlan::parse(&normalized, &owner.canonicalize()?.to_string_lossy())?;

    // Create snapshot directory structure
    let staging = manager.create_staging_dir(scope, snapshot_name)?;
    crate::volumes::verify_owned_volumes(executable, &capture_plan.volumes).await?;

    let snapshot_dir = staging.path().to_path_buf();
    let images_dir = snapshot_dir.join("images");
    let volumes_dir = snapshot_dir.join("volumes");
    let compose_dir = snapshot_dir.join("compose");

    for dir in [&snapshot_dir, &images_dir, &volumes_dir, &compose_dir] {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| VmError::filesystem(e, dir.to_string_lossy(), "create_dir_all"))?;
    }

    let (services, volumes) = {
        let paused = if quiesce {
            tracing::info!("Pausing containers for consistent snapshot...");
            crate::quiesce::pause(executable, &compose).await?
        } else {
            Vec::new()
        };

        let snapshot_result = async {
            let services = snapshot_compose_services(
                executable,
                &project_name,
                snapshot_name,
                &compose,
                &images_dir,
            )
            .await?;

            tracing::info!("Backing up volumes in parallel...");
            let volumes = backup_volumes(executable, &volumes_dir, &capture_plan.volumes).await?;
            Ok::<_, VmError>((services, volumes))
        }
        .await;

        let resume_result = if quiesce {
            tracing::info!("Unpausing containers...");
            crate::quiesce::resume(executable, &paused).await
        } else {
            Ok(())
        };
        crate::quiesce::finish(snapshot_result, resume_result)?
    };

    // Copy configuration files
    tracing::info!("Copying configuration files...");
    let vm_config_file = "vm.yaml";

    let restored_compose = snapshot_compose_configuration(&normalized, &services)?;
    tokio::fs::write(compose_dir.join(compose_file), restored_compose).await?;

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
        source_environment: source_environment.map(str::to_string),
        provider: executable.to_string(),
        architecture: vm_platform::platform::architecture().to_string(),
        consistency: if quiesce { "quiesced" } else { "live" }.to_string(),
        project_dir: project_dir.to_string_lossy().to_string(),
        owner_config_path: Some(owner.canonicalize()?.display().to_string()),
        git_commit,
        git_dirty,
        git_branch,
        services,
        volumes,
        native_vm_file: None,
        native_image_digest: None,
        native_runtime_fingerprint: None,
        excluded_mounts: capture_plan.excluded_mounts,
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
    compose: &ComposeProject,
    images_dir: &Path,
) -> Result<Vec<ServiceSnapshot>> {
    tracing::info!("Discovering services...");
    // Podman's Compose API excludes paused containers from the default query.
    // A quiesced snapshot must still capture their root filesystems.
    let services_output =
        execute_docker_compose(executable, &["ps", "--all", "--services"], compose).await?;
    let service_names: Vec<String> = services_output
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    if service_names.is_empty() {
        return Err(VmError::validation(
            "No container services are available to snapshot",
            None::<String>,
        ));
    }

    tracing::info!("Snapshotting services in parallel...");
    let futures = service_names.iter().map(|service| {
        let service = service.clone();
        let project_name = project_name.to_string();
        let snapshot_name = snapshot_name.to_string();
        let images_dir = images_dir.to_path_buf();
        let compose = compose.clone();
        async move {
            let container_id =
                execute_docker_compose(executable, &["ps", "--all", "-q", &service], &compose)
                    .await?;
            if container_id.is_empty() {
                return Err(VmError::validation(
                    format!("Service '{service}' disappeared while preparing the snapshot"),
                    None::<String>,
                ));
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
        }
    });
    stream::iter(futures)
        .buffer_unordered(optimal_concurrency())
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()
}

fn snapshot_compose_configuration(
    normalized: &str,
    services: &[ServiceSnapshot],
) -> Result<Vec<u8>> {
    let mut compose: serde_json::Value = serde_json::from_str(normalized)
        .map_err(|error| VmError::general(error, "Invalid normalized Compose configuration"))?;
    for service in services {
        let definition = compose["services"][&service.name]
            .as_object_mut()
            .ok_or_else(|| {
                VmError::validation(
                    "Captured service is absent from Compose configuration",
                    None::<String>,
                )
            })?;
        definition.insert(
            "image".into(),
            serde_json::Value::String(service.image_tag.clone()),
        );
        definition.remove("build");
        definition.remove("pull_policy");
    }
    serde_json::to_vec_pretty(&compose)
        .map_err(|error| VmError::general(error, "Cannot save snapshot Compose configuration"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    async fn snapshot_paused_services(mode: &str) -> Result<Vec<ServiceSnapshot>> {
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let runtime = directory.path().join("runtime");
        let script = r#"#!/bin/sh
if [ "$1" = compose ]; then
    # Model the runtime API: its default ps query excludes paused containers.
    case " $* " in *' --all '*) ;; *) exit 0 ;; esac
    case " $* " in
      *' --services '*) [ 'MODE' = empty ] || printf 'dev\n' ;;
      *) [ 'MODE' = disappeared ] || printf 'paused-container\n' ;;
    esac
    exit 0
fi
if [ "$1" = commit ]; then
    [ "$2" = paused-container ] || exit 1
    printf 'sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
    exit 0
fi
if [ "$1" = save ]; then
    printf 'captured-rootfs' > "$4"
    exit 0
fi
if [ "$1" = image ] && [ "$2" = rm ]; then
    case "$3" in vm-snapshot/owned/dev:clean-*) exit 0 ;; esac
fi
exit 1
"#;
        std::fs::write(&runtime, script.replace("MODE", mode)).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o755)).unwrap();
        let compose = ComposeProject {
            directory: directory.path().to_path_buf(),
            file: directory.path().join("compose.yaml"),
        };
        let result = snapshot_compose_services(
            runtime.to_str().unwrap(),
            "owned",
            "clean",
            &compose,
            directory.path(),
        )
        .await;
        if result.is_ok() {
            assert_eq!(
                std::fs::read(directory.path().join("dev.tar")).unwrap(),
                b"captured-rootfs"
            );
        }
        result
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn paused_service_rootfs_is_included_in_snapshot() {
        let services = snapshot_paused_services("paused").await.unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "dev");
        assert_eq!(services[0].image_file, "dev.tar");
        assert!(services[0].image_digest.is_some());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn empty_or_disappearing_services_cannot_produce_successful_snapshots() {
        let empty = snapshot_paused_services("empty").await.unwrap_err();
        assert!(empty.to_string().contains("No container services"));
        let disappeared = snapshot_paused_services("disappeared").await.unwrap_err();
        assert!(disappeared.to_string().contains("disappeared"));
    }

    #[test]
    fn saved_compose_restores_captured_rootfs_without_rebuilding_or_pulling() {
        let normalized = r#"{"name":"owned","services":{"dev":{"image":"base:latest","build":{"context":"/project"},"pull_policy":"always","volumes":[{"type":"bind","source":"/project","target":"/workspace"}]}}}"#;
        let captured = ServiceSnapshot {
            name: "dev".into(),
            image_tag: "vm-snapshot/owned/dev:clean".into(),
            image_file: "dev.tar".into(),
            image_digest: Some("sha256:captured".into()),
        };
        let saved: serde_json::Value = serde_json::from_slice(
            &snapshot_compose_configuration(normalized, &[captured]).unwrap(),
        )
        .unwrap();
        let service = &saved["services"]["dev"];
        assert_eq!(service["image"], "vm-snapshot/owned/dev:clean");
        assert!(service.get("build").is_none());
        assert!(service.get("pull_policy").is_none());
        assert_eq!(service["volumes"][0]["source"], "/project");
    }
}
