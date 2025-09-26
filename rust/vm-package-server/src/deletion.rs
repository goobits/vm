//! # Package Deletion Operations
//!
//! This module provides safe deletion operations for packages across different registry types.
//! It handles the complexities of package removal while maintaining registry integrity and
//! providing appropriate fallback mechanisms for different deletion strategies.
//!
//! ## Supported Operations
//!
//! - **File-based deletion**: Removes package files matching specific patterns (PyPI)
//! - **Metadata updates**: Updates JSON metadata to remove versions (NPM)
//! - **Index management**: Updates or removes entries from registry indexes (Cargo)
//! - **Yanking support**: Marks packages as yanked rather than fully deleting (Cargo)
//!
//! ## Security Considerations
//!
//! - Validates package names and versions before deletion
//! - Prevents accidental deletion of unrelated packages through pattern matching
//! - Maintains audit trails through structured logging
//! - Handles partial failures gracefully to prevent corrupted registry states
//!
//! ## Registry-Specific Behavior
//!
//! - **PyPI**: Removes `.whl` and `.tar.gz` files plus associated `.meta` files
//! - **NPM**: Updates `package.json` metadata, manages `dist-tags`, preserves history
//! - **Cargo**: Supports both yanking (marking as unavailable) and full deletion
//!
//! ## Usage Example
//!
//! ```rust
//! use vm_package_server::deletion::{remove_files_matching_pattern, DeletionOptions};
//! use std::path::Path;
//!
//! // Remove PyPI package files
//! let deleted = remove_files_matching_pattern(
//!     Path::new("/data/pypi/packages"),
//!     "my-package",
//!     "1.0.0",
//!     &[".whl", ".tar.gz"]
//! ).await?;
//! ```

use anyhow::Result;
use serde_json::Value;
use std::path::Path;
use tracing::{debug, info, warn};

/// Options for deletion operations
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct DeletionOptions {
    /// For Cargo: true = force delete, false = yank
    pub force: bool,
}

/// Remove files matching a specific pattern (for PyPI and similar)
/// Returns list of deleted files
#[allow(dead_code)]
pub async fn remove_files_matching_pattern(
    dir: &Path,
    package_name: &str,
    version: &str,
    extensions: &[&str],
) -> Result<Vec<String>> {
    let mut deleted_files = Vec::new();

    if !dir.exists() {
        return Ok(deleted_files);
    }

    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
            // Check if filename matches pattern: package_name-version-*
            // or package_name-version.ext
            let version_prefix = format!("{}-{}", package_name, version);
            if filename.starts_with(&version_prefix) {
                // Check if it has the right extension (if specified)
                let should_delete = if extensions.is_empty() {
                    true
                } else {
                    extensions.iter().any(|ext| filename.ends_with(ext))
                };

                if should_delete {
                    // Check if this is actually the version we want
                    // (avoid deleting 1.0.10 when we want 1.0.1)
                    let after_version = &filename[version_prefix.len()..];
                    if after_version.is_empty()
                        || after_version.starts_with('.')
                        || after_version.starts_with('-')
                    {
                        if let Err(e) = tokio::fs::remove_file(&path).await {
                            warn!(file = %path.display(), error = %e, "Failed to delete file");
                        } else {
                            debug!(file = %filename, "Deleted file");
                            deleted_files.push(filename.to_string());

                            // Also delete corresponding .meta file if it exists
                            let meta_extensions = [".whl.meta", ".tar.gz.meta"];
                            for meta_ext in meta_extensions {
                                if filename.ends_with(meta_ext.trim_end_matches(".meta")) {
                                    let meta_path = path.with_extension(format!(
                                        "{}.meta",
                                        path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
                                    ));
                                    if meta_path.exists() {
                                        let _ = tokio::fs::remove_file(&meta_path).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(deleted_files)
}

/// Update NPM metadata JSON to remove a specific version
#[allow(dead_code)]
pub async fn update_json_remove_version(metadata_path: &Path, version: &str) -> Result<()> {
    if !metadata_path.exists() {
        anyhow::bail!("Package metadata not found");
    }

    let content = tokio::fs::read_to_string(metadata_path).await?;
    let mut metadata: Value = serde_json::from_str(&content)?;

    // First, check if version exists and collect remaining versions
    let mut remaining_versions = Vec::new();
    let mut version_exists = false;

    if let Some(versions) = metadata.get("versions").and_then(|v| v.as_object()) {
        version_exists = versions.contains_key(version);
        for (ver, _) in versions.iter() {
            if ver != version {
                remaining_versions.push(ver.clone());
            }
        }
    }

    if !version_exists {
        anyhow::bail!("Version {} not found in package metadata", version);
    }

    // Remove the version from versions object
    if let Some(versions) = metadata.get_mut("versions").and_then(|v| v.as_object_mut()) {
        versions.remove(version);
        info!(version = %version, "Removed version from metadata");
    }

    // Update dist-tags if this was the latest version
    if let Some(dist_tags) = metadata
        .get_mut("dist-tags")
        .and_then(|v| v.as_object_mut())
    {
        if dist_tags.get("latest").and_then(|v| v.as_str()) == Some(version) {
            // Find the new latest version from remaining versions
            let mut latest_version: Option<String> = None;
            for ver in &remaining_versions {
                if latest_version.as_ref().is_none_or(|latest| ver > latest) {
                    latest_version = Some(ver.clone());
                }
            }

            if let Some(new_latest) = latest_version {
                dist_tags.insert("latest".to_string(), Value::String(new_latest.clone()));
                info!(new_latest = %new_latest, "Updated latest tag");
            } else {
                dist_tags.remove("latest");
                info!("Removed latest tag (no versions remaining)");
            }
        }
    }

    // Save updated metadata
    let updated_content = serde_json::to_string_pretty(&metadata)?;
    tokio::fs::write(metadata_path, updated_content).await?;
    Ok(())
}

/// Update Cargo index to yank a specific version
pub async fn update_index_yank_version(index_path: &Path, version: &str) -> Result<()> {
    if !index_path.exists() {
        anyhow::bail!("Crate index not found");
    }

    let content = tokio::fs::read_to_string(index_path).await?;
    let mut updated_lines = Vec::new();
    let mut found = false;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(mut entry) = serde_json::from_str::<Value>(line) {
            if entry.get("vers").and_then(|v| v.as_str()) == Some(version) {
                // Mark as yanked
                entry["yanked"] = Value::Bool(true);
                updated_lines.push(serde_json::to_string(&entry)?);
                found = true;
                info!(version = %version, "Marked version as yanked in index");
            } else {
                updated_lines.push(line.to_string());
            }
        } else {
            warn!(line = %line, "Failed to parse index line, keeping as-is");
            updated_lines.push(line.to_string());
        }
    }

    if !found {
        anyhow::bail!("Version {} not found in crate index", version);
    }

    // Write back the updated index
    let updated_content = updated_lines.join("\n");
    tokio::fs::write(index_path, updated_content).await?;
    Ok(())
}

/// Remove a specific version line from Cargo index completely
pub async fn remove_version_from_index(index_path: &Path, version: &str) -> Result<()> {
    if !index_path.exists() {
        anyhow::bail!("Crate index not found");
    }

    let content = tokio::fs::read_to_string(index_path).await?;
    let mut updated_lines = Vec::new();
    let mut found = false;

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<Value>(line) {
            if entry.get("vers").and_then(|v| v.as_str()) == Some(version) {
                // Skip this line (remove it)
                found = true;
                info!(version = %version, "Removed version from index");
            } else {
                updated_lines.push(line.to_string());
            }
        } else {
            warn!(line = %line, "Failed to parse index line, keeping as-is");
            updated_lines.push(line.to_string());
        }
    }

    if !found {
        anyhow::bail!("Version {} not found in crate index", version);
    }

    if updated_lines.is_empty() {
        // If no versions left, delete the index file
        tokio::fs::remove_file(index_path).await?;
        info!("Removed index file (no versions remaining)");
    } else {
        // Write back the updated index
        let updated_content = updated_lines.join("\n");
        tokio::fs::write(index_path, updated_content).await?;
    }

    Ok(())
}
