use std::collections::HashMap;
use std::path::Path;

use chrono::Utc;
use vm_core::command_stream::stream_command;
use vm_core::error::{Result, VmError};

use crate::archive::directory_size;
use crate::images::{image_digest, save_image_streaming};
use crate::manager::{SnapshotManager, SnapshotScope};
use crate::metadata::{ServiceSnapshot, SnapshotMetadata};

pub(crate) async fn create_from_dockerfile(
    executable: &str,
    name: &str,
    description: Option<&str>,
    dockerfile_path: &Path,
    build_context: &Path,
    build_args: &[String],
    force: bool,
) -> Result<()> {
    if !dockerfile_path.exists() {
        return Err(VmError::validation(
            format!("Dockerfile not found: {}", dockerfile_path.display()),
            None::<String>,
        ));
    }

    let snapshot_name = name.trim_start_matches('@');
    let scope = SnapshotScope::Global;
    let manager = SnapshotManager::new()?;
    if manager.snapshot_exists(scope, snapshot_name)? && !force {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' already exists globally. Use --force to overwrite.",
                snapshot_name
            ),
            None::<String>,
        ));
    }

    let image_tag = format!("vm-snapshot/global/{snapshot_name}:latest");
    tracing::info!("Building snapshot '{}' from Dockerfile...", name);
    if let Some(description) = description {
        tracing::info!("Description: {}", description);
    }

    let build_args = parse_build_args(build_args)?;
    tracing::info!("Build context: {}", build_context.display());
    tracing::info!("Dockerfile: {}", dockerfile_path.display());
    if !build_args.is_empty() {
        tracing::info!(
            build_arg_count = build_args.len(),
            "Docker build arguments configured"
        );
    }

    let mut args = vec![
        "build".to_string(),
        "-f".to_string(),
        dockerfile_path.to_string_lossy().to_string(),
        "-t".to_string(),
        image_tag.clone(),
    ];
    for (key, value) in &build_args {
        args.push("--build-arg".to_string());
        args.push(format!("{key}={value}"));
    }
    args.push(build_context.to_string_lossy().to_string());
    stream_command(executable, &args).map_err(|error| {
        VmError::general(
            error,
            format!(
                "Failed to build Docker image from {}",
                dockerfile_path.display()
            ),
        )
    })?;

    let staging = manager.create_staging_dir(scope, snapshot_name)?;
    let snapshot_dir = staging.path().to_path_buf();
    let images_dir = snapshot_dir.join("images");
    tokio::fs::create_dir_all(&images_dir)
        .await
        .map_err(|error| {
            VmError::filesystem(error, images_dir.to_string_lossy(), "create_dir_all")
        })?;

    tracing::info!("Saving snapshot to disk...");
    let image_file = "base.tar";
    save_image_streaming(executable, &image_tag, &images_dir.join(image_file)).await?;

    let project_dir = std::env::current_dir()
        .map_err(|error| VmError::filesystem(error, "current_dir", "get"))?;
    let total_size_bytes = directory_size(&snapshot_dir);
    SnapshotMetadata {
        name: snapshot_name.to_string(),
        created_at: Utc::now(),
        description: description.map(str::to_string),
        project_name: "global".to_string(),
        project_dir: project_dir.to_string_lossy().to_string(),
        git_commit: None,
        git_dirty: false,
        git_branch: None,
        services: vec![ServiceSnapshot {
            name: "base".to_string(),
            image_digest: image_digest(executable, &image_tag).await?,
            image_tag,
            image_file: image_file.to_string(),
        }],
        volumes: vec![],
        compose_file: String::new(),
        vm_config_file: String::new(),
        total_size_bytes,
    }
    .save(snapshot_dir.join("metadata.json"))?;
    manager.install_staged_snapshot(staging, scope, snapshot_name, force)?;

    tracing::info!(
        "Snapshot '{}' created successfully ({:.2} MB)",
        name,
        total_size_bytes as f64 / (1024.0 * 1024.0)
    );
    tracing::info!("\nYou can now use this snapshot in vm.yaml:");
    tracing::info!("  vm:");
    tracing::info!("    image: @{}", snapshot_name);
    Ok(())
}

fn parse_build_args(build_args: &[String]) -> Result<HashMap<String, String>> {
    build_args
        .iter()
        .map(|argument| {
            argument
                .split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .ok_or_else(|| {
                    VmError::validation(
                        format!("Invalid build arg format '{argument}'. Expected KEY=VALUE"),
                        None::<String>,
                    )
                })
        })
        .collect()
}
