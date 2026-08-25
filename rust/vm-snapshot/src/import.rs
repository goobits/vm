//! Snapshot import functionality

use crate::archive::{copy_directory, extract_gzip_archive, validate_snapshot_files};
use crate::archive_manifest::ArchiveManifest;
use crate::images::load_service_images;
use crate::manager::{SnapshotManager, SnapshotScope};
use crate::metadata::SnapshotMetadata;
use std::path::Path;
use vm_core::error::{Result, VmError};

/// Handle snapshot import
pub async fn handle_import(
    executable: &str,
    file_path: &Path,
    name_override: Option<&str>,
    force: bool,
) -> Result<()> {
    let manager = SnapshotManager::new()?;

    if !file_path.exists() {
        return Err(VmError::validation(
            format!("Snapshot file not found: {}", file_path.display()),
            None::<String>,
        ));
    }

    tracing::info!("Importing snapshot from {}...", file_path.display());

    // Create temp directory for extraction
    let temp_dir = tempfile::tempdir().map_err(|e| VmError::filesystem(e, "tempdir", "create"))?;
    let extract_dir = temp_dir.path().join("snapshot");
    tokio::fs::create_dir_all(&extract_dir)
        .await
        .map_err(|e| VmError::filesystem(e, extract_dir.display().to_string(), "create_dir_all"))?;

    tracing::info!("  Extracting archive...");

    extract_gzip_archive(file_path, &extract_dir)?;

    // Load manifest
    let manifest_path = extract_dir.join("manifest.json");
    if !manifest_path.is_file() {
        return Err(VmError::validation(
            "Invalid snapshot file: manifest.json not found",
            None::<String>,
        ));
    }

    let manifest_content = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|e| VmError::filesystem(e, manifest_path.display().to_string(), "read"))?;

    let manifest = ArchiveManifest::parse(&manifest_content)?;

    // Get snapshot name (from manifest or override)
    let snapshot_name = name_override
        .map(|s| s.to_string())
        .unwrap_or_else(|| manifest.snapshot_name().to_string());

    let is_global = manifest.is_global();
    let project_name = manifest.project_name();

    tracing::info!("  Snapshot name: {}", snapshot_name);
    tracing::info!("  Project: {}", project_name);
    tracing::info!(
        "  Type: {}",
        if is_global {
            "global (base image)"
        } else {
            "project-specific"
        }
    );

    // Check if snapshot already exists
    let scope = if is_global {
        SnapshotScope::Global
    } else {
        SnapshotScope::Project(project_name)
    };

    if manager.snapshot_exists(scope, &snapshot_name)? && !force {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' already exists for project '{}'. Use --force to overwrite.",
                snapshot_name, project_name
            ),
            None::<String>,
        ));
    }

    manifest.validate_current_platform()?;

    // Load snapshot metadata
    let metadata_path = extract_dir.join("metadata.json");
    if !metadata_path.is_file() {
        return Err(VmError::validation(
            "Invalid snapshot file: metadata.json not found",
            None::<String>,
        ));
    }

    let metadata = SnapshotMetadata::load(&metadata_path)?;
    validate_import_contents(&manifest, &metadata, &extract_dir)?;

    tracing::info!("  Loading Docker images...");

    let images_dir = extract_dir.join("images");
    if images_dir.exists() {
        tracing::info!("Loading service images in parallel...");
        load_service_images(executable, &images_dir, &metadata.services).await?;
    }

    tracing::info!("  Installing snapshot...");
    let staging = manager.create_staging_dir(scope, &snapshot_name)?;
    copy_directory(&extract_dir, staging.path()).await?;
    manager.install_staged_snapshot(staging, scope, &snapshot_name, force)?;

    tracing::info!("Snapshot '{}' imported successfully!", snapshot_name);

    if is_global {
        tracing::info!("\nTo use this base image in any project:");
        tracing::info!("  1. Add to your vm.yaml:");
        tracing::info!("     vm:");
        tracing::info!("       image: @{}", snapshot_name);
        tracing::info!("  2. Run: vm run linux");
        tracing::info!("\nThe VM will start instantly using the imported base image!");
    } else {
        tracing::info!("\nTo restore this project snapshot:");
        tracing::info!("  vm revert {}", snapshot_name);
    }

    Ok(())
}

fn validate_import_contents(
    manifest: &ArchiveManifest,
    metadata: &SnapshotMetadata,
    extract_dir: &Path,
) -> Result<()> {
    let expected_project = manifest.project_name();

    if metadata.project_name != expected_project {
        return Err(VmError::validation(
            format!(
                "Snapshot metadata project '{}' does not match manifest project '{}'",
                metadata.project_name, expected_project
            ),
            None::<String>,
        ));
    }

    validate_snapshot_files(extract_dir, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{ServiceSnapshot, SnapshotMetadata, VolumeSnapshot};

    #[test]
    fn validate_import_contents_rejects_missing_image() {
        let tempdir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tempdir.path().join("images")).unwrap();
        std::fs::create_dir_all(tempdir.path().join("volumes")).unwrap();

        let manifest = ArchiveManifest::parse(
            r#"{
                "version": "1.0",
                "snapshot_name": "demo",
                "is_global": true,
                "project_name": "global"
            }"#,
        )
        .unwrap();
        let metadata = SnapshotMetadata {
            name: "demo".to_string(),
            created_at: chrono::Utc::now(),
            description: None,
            project_name: "global".to_string(),
            project_dir: ".".to_string(),
            git_commit: None,
            git_dirty: false,
            git_branch: None,
            services: vec![ServiceSnapshot {
                name: "base".to_string(),
                image_tag: "demo:latest".to_string(),
                image_file: "base.tar".to_string(),
                image_digest: None,
            }],
            volumes: vec![VolumeSnapshot {
                name: "cache".to_string(),
                archive_file: "cache.tar.zst".to_string(),
                size_bytes: 1,
            }],
            compose_file: String::new(),
            vm_config_file: String::new(),
            total_size_bytes: 0,
        };

        let err = validate_import_contents(&manifest, &metadata, tempdir.path()).unwrap_err();
        assert!(err.to_string().contains("base.tar"));
    }
}
