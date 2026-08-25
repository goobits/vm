//! Snapshot export functionality

use crate::archive::{copy_directory, create_gzip_archive, validate_snapshot_files};
use crate::archive_manifest::ArchiveManifest;
use crate::manager::{SnapshotManager, SnapshotScope};
use crate::metadata::SnapshotMetadata;
use std::path::Path;
use vm_core::error::{Result, VmError};

/// Handle snapshot export
pub async fn handle_export(
    executable: &str,
    name: &str,
    output_path: Option<&Path>,
    compress_level: u8,
    project_override: Option<&str>,
) -> Result<()> {
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
    let scope = if is_global {
        SnapshotScope::Global
    } else {
        SnapshotScope::Project(&project_name)
    };

    if !manager.snapshot_exists(scope, clean_name)? {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' not found for project '{}'",
                clean_name, project_name
            ),
            None::<String>,
        ));
    }

    tracing::info!(
        "Exporting snapshot '{}' from project '{}'...",
        clean_name,
        project_name
    );

    // Load snapshot metadata
    let snapshot_dir = manager.get_snapshot_dir(scope, clean_name)?;
    let metadata_path = snapshot_dir.join("metadata.json");
    let metadata = SnapshotMetadata::load(&metadata_path)?;
    validate_snapshot_files(&snapshot_dir, &metadata)?;

    // Determine output file path
    let output_file = output_path.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join(format!("{}.snapshot.tar.gz", clean_name))
    });

    tracing::info!("  Creating export tarball...");

    // Create temp directory for export
    let temp_dir = tempfile::tempdir().map_err(|e| VmError::filesystem(e, "tempdir", "create"))?;
    let export_build_dir = temp_dir.path().join("snapshot");
    tokio::fs::create_dir_all(&export_build_dir)
        .await
        .map_err(|e| {
            VmError::filesystem(e, export_build_dir.display().to_string(), "create_dir_all")
        })?;

    // Create manifest.json
    let manifest = ArchiveManifest::new(executable, clean_name, is_global, &metadata);

    let manifest_path = export_build_dir.join("manifest.json");
    let manifest_json = manifest.to_json_pretty()?;
    tokio::fs::write(&manifest_path, manifest_json)
        .await
        .map_err(|e| VmError::filesystem(e, manifest_path.display().to_string(), "write"))?;

    // Export the immutable image archives recorded by snapshot creation.
    let images_dir = export_build_dir.join("images");
    tracing::info!("Copying recorded service images...");
    copy_directory(&snapshot_dir.join("images"), &images_dir).await?;

    // Copy metadata.json
    let metadata_dest = export_build_dir.join("metadata.json");
    tokio::fs::copy(&metadata_path, &metadata_dest)
        .await
        .map_err(|e| VmError::filesystem(e, metadata_dest.display().to_string(), "copy"))?;

    // Copy volumes if they exist
    let volumes_src = snapshot_dir.join("volumes");
    if volumes_src.exists() {
        let volumes_dest = export_build_dir.join("volumes");
        copy_directory(&volumes_src, &volumes_dest).await?;
    }

    // Copy compose files if they exist
    let compose_src = snapshot_dir.join("compose");
    if compose_src.exists() {
        let compose_dest = export_build_dir.join("compose");
        copy_directory(&compose_src, &compose_dest).await?;
    }

    tracing::info!("  Compressing snapshot...");

    create_gzip_archive(&export_build_dir, &output_file, compress_level)?;

    // Get final file size
    let file_size = std::fs::metadata(&output_file)
        .map_err(|error| VmError::filesystem(error, output_file.display(), "metadata"))?
        .len();

    tracing::info!("Snapshot exported successfully: {}", output_file.display());
    tracing::info!("  Size: {:.2} MB", file_size as f64 / (1024.0 * 1024.0));

    if is_global {
        tracing::info!("\nTo import on another machine:");
        tracing::info!("  vm import {}", output_file.display());
        tracing::info!("\nThen use in any project with:");
        tracing::info!("  vm:");
        tracing::info!("    image: @{}", clean_name);
    } else {
        tracing::info!("\nTo import on another machine:");
        tracing::info!("  vm import {}", output_file.display());
    }

    Ok(())
}
