//! Snapshot import functionality

use crate::docker::execute_docker_streaming;
use crate::manager::{SnapshotManager, SnapshotScope};
use crate::metadata::SnapshotMetadata;
use crate::optimal_concurrency;
use futures::stream::{self, StreamExt};
use std::path::Path;
use vm_core::error::{Result, VmError};
use vm_core::{vm_println, vm_success, vm_warning};

/// Handle snapshot import
pub async fn handle_import(
    executable: &str,
    file_path: &Path,
    name_override: Option<&str>,
    verify: bool,
    force: bool,
) -> Result<()> {
    let manager = SnapshotManager::new()?;

    if !file_path.exists() {
        return Err(VmError::validation(
            format!("Snapshot file not found: {}", file_path.display()),
            None::<String>,
        ));
    }

    vm_println!("Importing snapshot from {}...", file_path.display());

    // Create temp directory for extraction
    let temp_dir = tempfile::tempdir().map_err(|e| VmError::filesystem(e, "tempdir", "create"))?;
    let extract_dir = temp_dir.path().join("snapshot");
    tokio::fs::create_dir_all(&extract_dir)
        .await
        .map_err(|e| VmError::filesystem(e, extract_dir.display().to_string(), "create_dir_all"))?;

    vm_println!("  Extracting archive...");

    // Extract tarball
    let tar_gz = std::fs::File::open(file_path)
        .map_err(|e| VmError::filesystem(e, file_path.display().to_string(), "open"))?;

    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);

    archive
        .unpack(&extract_dir)
        .map_err(|e| VmError::general(e, "Failed to extract tar archive"))?;

    // Load manifest
    let manifest_path = extract_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(VmError::validation(
            "Invalid snapshot file: manifest.json not found",
            None::<String>,
        ));
    }

    let manifest_content = tokio::fs::read_to_string(&manifest_path)
        .await
        .map_err(|e| VmError::filesystem(e, manifest_path.display().to_string(), "read"))?;

    let manifest: serde_json::Value = serde_json::from_str(&manifest_content)
        .map_err(|e| VmError::general(e, "Failed to parse manifest.json"))?;

    // Get snapshot name (from manifest or override)
    let snapshot_name = name_override
        .map(|s| s.to_string())
        .or_else(|| {
            manifest
                .get("snapshot_name")
                .and_then(|v| v.as_str())
                .map(String::from)
        })
        .ok_or_else(|| {
            VmError::validation(
                "Snapshot name not found in manifest and no override provided",
                None::<String>,
            )
        })?;

    let is_global = manifest
        .get("is_global")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let project_name = if is_global {
        "global"
    } else {
        manifest
            .get("project_name")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
    };

    vm_println!("  Snapshot name: {}", snapshot_name);
    vm_println!("  Project: {}", project_name);
    vm_println!(
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

    if manager.snapshot_exists(scope, &snapshot_name) && !force {
        return Err(VmError::validation(
            format!(
                "Snapshot '{}' already exists for project '{}'. Use --force to overwrite.",
                snapshot_name, project_name
            ),
            None::<String>,
        ));
    }

    // Verify platform compatibility (warning only)
    if verify {
        validate_manifest_platform(&manifest)?;
    }

    // Load snapshot metadata
    let metadata_path = extract_dir.join("metadata.json");
    if !metadata_path.exists() {
        return Err(VmError::validation(
            "Invalid snapshot file: metadata.json not found",
            None::<String>,
        ));
    }

    let metadata = SnapshotMetadata::load(&metadata_path)?;
    validate_import_contents(&manifest, &metadata, &extract_dir)?;

    vm_println!("  Loading Docker images...");

    // Load Docker images
    let images_dir = extract_dir.join("images");
    if images_dir.exists() {
        // Parallelize image loading for 2-5x faster import
        vm_println!("Loading service images in parallel...");
        let load_futures = metadata.services.iter().map(|service| {
            let service_name = service.name.clone();
            let image_file = service.image_file.clone();
            let images_dir = images_dir.clone();

            async move {
                let image_path = images_dir.join(&image_file);

                if !image_path.exists() {
                    vm_warning!("Image file '{}' not found, skipping", image_file);
                    return Ok::<(), VmError>(());
                }

                vm_println!("    Loading image for service '{}'...", service_name);

                let image_path_str = image_path.to_str().ok_or_else(|| {
                    VmError::general(
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Invalid UTF-8 in path",
                        ),
                        format!(
                            "Snapshot path contains invalid UTF-8 characters: {}",
                            image_path.display()
                        ),
                    )
                })?;
                // Stream output so users see "Loaded image: ..." progress
                execute_docker_streaming(executable, &["load", "-i", image_path_str]).await?;
                Ok(())
            }
        });

        // Load images concurrently (CPU-adaptive concurrency)
        stream::iter(load_futures)
            .buffer_unordered(optimal_concurrency())
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
    }

    // Create snapshot directory
    let snapshot_dir = manager.get_snapshot_dir(scope, &snapshot_name);

    if snapshot_dir.exists() && force {
        vm_println!("  Removing existing snapshot...");
        tokio::fs::remove_dir_all(&snapshot_dir)
            .await
            .map_err(|e| {
                VmError::filesystem(e, snapshot_dir.display().to_string(), "remove_dir_all")
            })?;
    }

    vm_println!("  Installing snapshot...");

    tokio::fs::create_dir_all(&snapshot_dir)
        .await
        .map_err(|e| {
            VmError::filesystem(e, snapshot_dir.display().to_string(), "create_dir_all")
        })?;

    // Copy all snapshot contents
    copy_dir_all(&extract_dir, &snapshot_dir).await?;

    vm_success!("Snapshot '{}' imported successfully!", snapshot_name);

    if is_global {
        vm_println!("\nTo use this base image in any project:");
        vm_println!("  1. Add to your vm.yaml:");
        vm_println!("     vm:");
        vm_println!("       box: @{}", snapshot_name);
        vm_println!("  2. Run: vm create");
        vm_println!("\nThe VM will start instantly using the imported base image!");
    } else {
        vm_println!("\nTo restore this project snapshot:");
        vm_println!("  vm snapshot restore {}", snapshot_name);
    }

    Ok(())
}

fn validate_manifest_platform(manifest: &serde_json::Value) -> Result<()> {
    vm_println!("  Verifying platform compatibility...");
    let current_arch = std::env::consts::ARCH;
    let current_os = std::env::consts::OS;
    vm_println!("    Current platform: {}/{}", current_os, current_arch);

    let manifest_os = manifest
        .get("platform")
        .and_then(|platform| platform.get("os"))
        .and_then(|value| value.as_str());
    let manifest_arch = manifest
        .get("platform")
        .and_then(|platform| platform.get("arch"))
        .and_then(|value| value.as_str());

    if let (Some(manifest_os), Some(manifest_arch)) = (manifest_os, manifest_arch) {
        if manifest_os != current_os || manifest_arch != current_arch {
            return Err(VmError::validation(
                format!(
                    "Snapshot was exported for {}/{} but current platform is {}/{}",
                    manifest_os, manifest_arch, current_os, current_arch
                ),
                Some(
                    "Use a matching machine or re-export the snapshot on this platform".to_string(),
                ),
            ));
        }
    } else {
        vm_warning!("Snapshot manifest does not include platform metadata; proceeding without a compatibility guarantee.");
    }

    Ok(())
}

fn validate_import_contents(
    manifest: &serde_json::Value,
    metadata: &SnapshotMetadata,
    extract_dir: &Path,
) -> Result<()> {
    let is_global = manifest
        .get("is_global")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let expected_project = if is_global {
        "global"
    } else {
        manifest
            .get("project_name")
            .and_then(|value| value.as_str())
            .unwrap_or("default")
    };

    if metadata.project_name != expected_project {
        return Err(VmError::validation(
            format!(
                "Snapshot metadata project '{}' does not match manifest project '{}'",
                metadata.project_name, expected_project
            ),
            None::<String>,
        ));
    }

    let images_dir = extract_dir.join("images");
    for service in &metadata.services {
        let image_path = images_dir.join(&service.image_file);
        if !image_path.exists() {
            return Err(VmError::validation(
                format!("Snapshot image file is missing: {}", image_path.display()),
                None::<String>,
            ));
        }
    }

    let volumes_dir = extract_dir.join("volumes");
    for volume in &metadata.volumes {
        let archive_path = volumes_dir.join(&volume.archive_file);
        if !archive_path.exists() {
            return Err(VmError::validation(
                format!(
                    "Snapshot volume archive is missing: {}",
                    archive_path.display()
                ),
                None::<String>,
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{ServiceSnapshot, SnapshotMetadata, VolumeSnapshot};

    #[test]
    fn validate_manifest_platform_accepts_matching_platform() {
        let manifest = serde_json::json!({
            "platform": {
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH
            }
        });

        assert!(validate_manifest_platform(&manifest).is_ok());
    }

    #[test]
    fn validate_import_contents_rejects_missing_image() {
        let tempdir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tempdir.path().join("images")).unwrap();
        std::fs::create_dir_all(tempdir.path().join("volumes")).unwrap();

        let manifest = serde_json::json!({
            "is_global": true,
            "project_name": "global"
        });
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

/// Recursively copy a directory
async fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dst)
        .await
        .map_err(|e| VmError::filesystem(e, dst.display().to_string(), "create_dir_all"))?;

    let mut entries = tokio::fs::read_dir(src)
        .await
        .map_err(|e| VmError::filesystem(e, src.display().to_string(), "read_dir"))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| VmError::general(e, "Failed to read directory entry"))?
    {
        let path = entry.path();
        let file_name = entry.file_name();

        // Skip the root directory itself
        if file_name == "snapshot" {
            continue;
        }

        let dest_path = dst.join(&file_name);

        if path.is_dir() {
            Box::pin(copy_dir_all(&path, &dest_path)).await?;
        } else {
            tokio::fs::copy(&path, &dest_path)
                .await
                .map_err(|e| VmError::filesystem(e, dest_path.display().to_string(), "copy"))?;
        }
    }

    Ok(())
}
