//! Snapshot export functionality

use super::manager::SnapshotManager;
use super::metadata::SnapshotMetadata;
use crate::error::{VmError, VmResult};
use futures::stream::{self, StreamExt};
use std::path::Path;
use vm_core::{vm_error, vm_println, vm_success};

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

/// Handle snapshot export
pub async fn handle_export(
    name: &str,
    output_path: Option<&Path>,
    compress_level: u8,
    project_override: Option<&str>,
) -> VmResult<()> {
    let manager = SnapshotManager::new()?;

    // Parse snapshot name to determine if it's global (@name) or project-specific
    let (is_global, clean_name) = if let Some(stripped) = name.strip_prefix('@') {
        (true, stripped)
    } else {
        (false, name)
    };

    let project_name = if is_global {
        "global".to_string()
    } else {
        project_override.map(|s| s.to_string()).unwrap_or_else(|| {
            std::env::current_dir()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_else(|| "default".to_string())
        })
    };

    // Check if snapshot exists
    if !manager.snapshot_exists(Some(&project_name), clean_name) {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' not found for project '{}'",
                clean_name, project_name
            ),
            None::<String>,
        ));
    }

    vm_println!(
        "Exporting snapshot '{}' from project '{}'...",
        clean_name,
        project_name
    );

    // Load snapshot metadata
    let snapshot_dir = manager.get_snapshot_dir(Some(&project_name), clean_name);
    let metadata_path = snapshot_dir.join("metadata.json");
    let metadata = SnapshotMetadata::load(&metadata_path)?;

    // Determine output file path
    let output_file = output_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(format!("{}.snapshot.tar.gz", clean_name))
    });

    vm_println!("  Creating export tarball...");

    // Create temp directory for export
    let temp_dir = tempfile::tempdir().map_err(|e| VmError::filesystem(e, "tempdir", "create"))?;
    let export_build_dir = temp_dir.path().join("snapshot");
    std::fs::create_dir_all(&export_build_dir).map_err(|e| {
        VmError::filesystem(e, export_build_dir.display().to_string(), "create_dir_all")
    })?;

    // Create manifest.json
    let manifest = serde_json::json!({
        "version": "1.0",
        "snapshot_name": clean_name,
        "is_global": is_global,
        "created_at": metadata.created_at,
        "description": metadata.description,
        "project_name": metadata.project_name,
        "total_size_bytes": metadata.total_size_bytes,
        "services": metadata.services.len(),
        "volumes": metadata.volumes.len(),
    });

    let manifest_path = export_build_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| VmError::general(e, "Failed to serialize manifest"))?;
    std::fs::write(&manifest_path, manifest_json)
        .map_err(|e| VmError::filesystem(e, manifest_path.display().to_string(), "write"))?;

    // Create images directory and export Docker images
    let images_dir = export_build_dir.join("images");
    std::fs::create_dir_all(&images_dir)
        .map_err(|e| VmError::filesystem(e, images_dir.display().to_string(), "create_dir_all"))?;

    // Parallelize image exports for 2-4x faster operation
    vm_println!("Exporting service images in parallel...");
    let export_futures = metadata.services.iter().map(|service| {
        let service_name = service.name.clone();
        let image_tag = service.image_tag.clone();
        let image_file = service.image_file.clone();
        let image_digest = service.image_digest.clone();
        let images_dir = images_dir.clone();

        async move {
            vm_println!("  Exporting image for service '{}'...", service_name);

            // Check if image exists
            let current_digest =
                execute_docker(&["image", "inspect", "--format={{.Id}}", &image_tag])
                    .await
                    .ok();

            if current_digest.is_none() {
                vm_error!(
                    "Image '{}' not found, skipping. You may need to recreate this snapshot.",
                    image_tag
                );
                return Ok::<(), VmError>(());
            }

            // Check if export already exists with same digest (deduplication)
            let image_export_path = images_dir.join(&image_file);
            if image_export_path.exists()
                && matches!((&image_digest, &current_digest), (Some(stored), Some(current)) if stored == current)
            {
                vm_println!(
                    "  Image '{}' unchanged (digest matches), skipping export",
                    service_name
                );
                return Ok::<(), VmError>(());
            }

            // Export image
            let image_path_str = image_export_path.to_str()
                .ok_or_else(|| VmError::general(
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid path"),
                    format!("Image export path contains invalid UTF-8: {}", image_export_path.display())
                ))?;

            let save_args = [
                "save",
                &image_tag,
                "-o",
                image_path_str,
            ];

            execute_docker(&save_args).await?;
            Ok(())
        }
    });

    // Export up to 3 images concurrently
    stream::iter(export_futures)
        .buffer_unordered(3)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<VmResult<Vec<_>>>()?;

    // Copy metadata.json
    let metadata_dest = export_build_dir.join("metadata.json");
    std::fs::copy(&metadata_path, &metadata_dest)
        .map_err(|e| VmError::filesystem(e, metadata_dest.display().to_string(), "copy"))?;

    // Copy volumes if they exist
    let volumes_src = snapshot_dir.join("volumes");
    if volumes_src.exists() {
        let volumes_dest = export_build_dir.join("volumes");
        copy_dir_all(&volumes_src, &volumes_dest)?;
    }

    // Copy compose files if they exist
    let compose_src = snapshot_dir.join("compose");
    if compose_src.exists() {
        let compose_dest = export_build_dir.join("compose");
        copy_dir_all(&compose_src, &compose_dest)?;
    }

    vm_println!("  Compressing snapshot...");

    // Create compressed tarball
    let tar_gz = std::fs::File::create(&output_file)
        .map_err(|e| VmError::filesystem(e, output_file.display().to_string(), "create"))?;

    let enc =
        flate2::write::GzEncoder::new(tar_gz, flate2::Compression::new(compress_level as u32));

    let mut tar = tar::Builder::new(enc);

    tar.append_dir_all(".", &export_build_dir)
        .map_err(|e| VmError::general(e, "Failed to create tar archive"))?;

    tar.finish()
        .map_err(|e| VmError::general(e, "Failed to finish tar archive"))?;

    // Get final file size
    let file_size = std::fs::metadata(&output_file)
        .map(|m| m.len())
        .unwrap_or(0);

    vm_success!("Snapshot exported successfully: {}", output_file.display());
    vm_println!("  Size: {:.2} MB", file_size as f64 / (1024.0 * 1024.0));

    if is_global {
        vm_println!("\nTo import on another machine:");
        vm_println!("  vm snapshot import {}", output_file.display());
        vm_println!("\nThen use in any project with:");
        vm_println!("  vm.box: @{}", clean_name);
    } else {
        vm_println!("\nTo import on another machine:");
        vm_println!("  vm snapshot import {}", output_file.display());
    }

    Ok(())
}

/// Recursively copy a directory
fn copy_dir_all(src: &Path, dst: &Path) -> VmResult<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| VmError::filesystem(e, dst.display().to_string(), "create_dir_all"))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| VmError::filesystem(e, src.display().to_string(), "read_dir"))?
    {
        let entry = entry.map_err(|e| VmError::general(e, "Failed to read directory entry"))?;
        let path = entry.path();
        let dest_path = dst.join(entry.file_name());

        if path.is_dir() {
            copy_dir_all(&path, &dest_path)?;
        } else {
            std::fs::copy(&path, &dest_path)
                .map_err(|e| VmError::filesystem(e, dest_path.display().to_string(), "copy"))?;
        }
    }

    Ok(())
}
