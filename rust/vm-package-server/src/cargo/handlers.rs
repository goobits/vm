//! Cargo HTTP endpoint handlers
//!
//! This module contains all HTTP endpoint handlers for the Cargo registry,
//! including package uploads, downloads, version management, and configuration.

use super::{index::*, parsing::*, storage::*};
use crate::deletion::{remove_version_from_index, update_index_yank_version};
use crate::{
    package_utils, sha256_hash, storage, validation, AppError, AppResult, AppState, SuccessResponse,
};
use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Count total number of Cargo crates
#[allow(dead_code)]
pub async fn count_crates(state: &AppState) -> AppResult<usize> {
    let index_dir = state.data_dir.join("cargo/index");

    package_utils::count_recursive_unique(index_dir, |path| {
        if path.is_file() {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.to_string())
        } else {
            None
        }
    })
    .await
}

/// List all crate names
#[allow(dead_code)]
pub async fn list_all_crates(state: &AppState) -> AppResult<Vec<String>> {
    let index_dir = state.data_dir.join("cargo/index");

    package_utils::list_recursive_unique(index_dir, |path| {
        if path.is_file() {
            path.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.to_string())
        } else {
            None
        }
    })
    .await
}

/// Get crate versions with checksums and file sizes
pub async fn get_crate_versions(
    state: &AppState,
    crate_name: &str,
) -> AppResult<Vec<(String, String, u64)>> {
    let index_path_str = index_path(crate_name)?;
    let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);
    let mut versions = Vec::new();

    if let Ok(content) = storage::read_file_string(&index_file_path).await {
        for line in content.lines() {
            if let Ok(entry) = serde_json::from_str::<Value>(line) {
                if let (Some(version), Some(cksum)) =
                    (entry["vers"].as_str(), entry["cksum"].as_str())
                {
                    // Get crate file size
                    let filename = format!("{crate_name}-{version}.crate");

                    // Validate the constructed filename for security
                    if let Err(e) = validation::validate_safe_path(&filename) {
                        warn!(filename = %filename, error = %e, "Skipping invalid filename in index");
                        continue;
                    }

                    let file_path = state.data_dir.join("cargo/crates").join(&filename);
                    let size = package_utils::get_file_size(&file_path).await;

                    versions.push((version.to_string(), cksum.to_string(), size));
                }
            }
        }
    }

    Ok(versions)
}

/// API endpoint to get versions for a specific crate
pub async fn get_crate_versions_api(
    AxumPath(crate_name): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<String>>> {
    debug!(crate_name = %crate_name, "Getting crate versions via API");
    let versions_data = get_crate_versions(&state, &crate_name).await?;

    // Extract just the version strings
    let versions: Vec<String> = versions_data
        .into_iter()
        .map(|(version, _, _)| version)
        .collect();

    Ok(Json(versions))
}

/// Get recent crates
pub async fn get_recent_crates(state: &AppState, limit: usize) -> AppResult<Vec<(String, String)>> {
    let crates_dir = state.data_dir.join("cargo/crates");

    package_utils::get_recent_packages_by_name_extraction(
        crates_dir,
        &[".crate"],
        limit,
        |filename| {
            // Extract crate name and version from filename (cratename-version.crate)
            if let Some(base) = filename.strip_suffix(".crate") {
                // Find the last hyphen to split name and version
                if let Some(last_dash) = base.rfind('-') {
                    let crate_name = &base[..last_dash];
                    let version = &base[last_dash + 1..];
                    Some((crate_name.to_string(), version.to_string()))
                } else {
                    None
                }
            } else {
                None
            }
        },
    )
    .await
}

/// Returns Cargo registry configuration required for client setup.
pub async fn config(State(state): State<Arc<AppState>>) -> AppResult<Json<Value>> {
    debug!("Incoming Cargo config request");
    let host = &state.server_addr;

    Ok(Json(json!({
        "dl": format!("{}/cargo/api/v1/crates/{{crate}}/{{version}}/download", host),
        "api": format!("{}/cargo", host)
    })))
}

/// Downloads Cargo crate files from local storage or upstream registry.
pub async fn download_crate(
    AxumPath((crate_name, version)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    debug!(crate_name = %crate_name, version = %version, "Incoming Cargo crate download request");

    // Validate crate name and version for security
    validation::validate_package_name(&crate_name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{crate_name}': {e}")))?;
    validation::validate_version(&version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version '{version}': {e}")))?;

    info!(crate_name = %crate_name, version = %version, "Downloading Cargo crate");
    let filename = format!("{crate_name}-{version}.crate");

    // Validate the constructed filename for security
    validation::validate_safe_path(&filename).map_err(|e| {
        AppError::BadRequest(format!("Generated unsafe filename '{filename}': {e}"))
    })?;

    let file_path = state.data_dir.join("cargo/crates").join(&filename);

    // Try local file first
    match storage::read_file(&file_path).await {
        Ok(data) => {
            debug!(crate_name = %crate_name, version = %version, size = data.len(), "Serving crate from local storage");
            Ok(data)
        }
        Err(_) => {
            // File not found locally, try upstream crates.io
            debug!(crate_name = %crate_name, version = %version, "Crate not found locally, checking upstream crates.io");
            match state
                .upstream_client
                .stream_cargo_crate(&crate_name, &version)
                .await
            {
                Ok(bytes) => {
                    info!(crate_name = %crate_name, version = %version, size = bytes.len(), "Streaming crate from upstream crates.io");
                    Ok(bytes.to_vec())
                }
                Err(e) => {
                    debug!(crate_name = %crate_name, version = %version, error = %e, "Crate not found on upstream crates.io either");
                    Err(e)
                }
            }
        }
    }
}

/// Publishes a new Cargo crate version to the local registry.
pub async fn publish_crate(
    State(state): State<Arc<AppState>>,
    body: axum::body::Bytes,
) -> AppResult<Json<SuccessResponse>> {
    debug!(payload_size = body.len(), "Incoming Cargo publish request");
    info!("Processing Cargo crate publish");

    // Parse and validate the upload payload
    let (metadata, crate_data) = parse_crate_upload(body)?;

    info!(crate_name = %metadata.name, version = %metadata.version, "Publishing Cargo crate");

    // Save the crate file
    save_crate_file(
        &crate_data,
        &metadata.name,
        &metadata.version,
        &state.data_dir,
    )
    .await?;

    // Calculate checksum
    let cksum = sha256_hash(&crate_data);

    // Update the index
    update_crate_index(&metadata, &cksum, &state.data_dir).await?;

    info!(
        crate_name = %metadata.name,
        version = %metadata.version,
        checksum = %cksum,
        "Cargo crate published successfully"
    );
    Ok(Json(SuccessResponse {
        message: "Crate published successfully".to_string(),
    }))
}

#[derive(Deserialize)]
pub struct DeleteParams {
    #[serde(default)]
    force: bool,
}

/// Yanks or deletes a specific version of a Cargo crate.
pub async fn delete_crate_version(
    AxumPath((crate_name, version)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
    Query(params): Query<DeleteParams>,
) -> AppResult<Json<SuccessResponse>> {
    // Validate crate name and version for security
    validation::validate_package_name(&crate_name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{crate_name}': {e}")))?;
    validation::validate_version(&version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version '{version}': {e}")))?;

    let action = if params.force { "Deleting" } else { "Yanking" };
    info!(crate_name = %crate_name, version = %version, action = action, "Processing Cargo crate");

    let index_path_str = index_path(&crate_name)?;
    let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);

    if params.force {
        // Force delete: remove from index and delete .crate file
        let crate_file = state
            .data_dir
            .join("cargo/crates")
            .join(format!("{crate_name}-{version}.crate"));

        // Remove the specific version from index
        remove_version_from_index(&index_file_path, &version).await?;

        // Delete the .crate file
        if crate_file.exists() {
            tokio::fs::remove_file(&crate_file).await?;
            info!(crate_file = %crate_file.display(), "Deleted crate file");
        }

        Ok(Json(SuccessResponse {
            message: format!("Force deleted version {version} of crate '{crate_name}'"),
        }))
    } else {
        // Yank: just mark as yanked in index
        update_index_yank_version(&index_file_path, &version).await?;

        Ok(Json(SuccessResponse {
            message: format!("Yanked version {version} of crate '{crate_name}'"),
        }))
    }
}

/// Placeholder API endpoint for crates.io compatibility
pub async fn crates_io_api_placeholder() -> AppResult<Response> {
    // Return minimal valid response for Cargo's API version check
    Ok((StatusCode::OK, Json(json!({"api_version": "v1"}))).into_response())
}

/// Deletes all versions of a Cargo crate from the registry.
pub async fn delete_all_versions(
    AxumPath(crate_name): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<SuccessResponse>> {
    // Validate crate name for security
    validation::validate_package_name(&crate_name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{crate_name}': {e}")))?;

    info!(crate_name = %crate_name, "Deleting all Cargo crate versions");

    let crates_dir = state.data_dir.join("cargo/crates");
    let index_dir = state.data_dir.join("cargo/index");

    let mut deleted_files = Vec::new();

    // Delete crate files (find all .crate files that start with crate name)
    if let Ok(entries) = std::fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                // Check if filename starts with crate name followed by a hyphen
                if filename.starts_with(&format!("{crate_name}-")) && filename.ends_with(".crate") {
                    if let Err(e) = std::fs::remove_file(&path) {
                        warn!(file = %path.display(), error = %e, "Failed to delete crate file");
                    } else {
                        deleted_files.push(filename.to_string());
                    }
                }
            }
        }
    }

    // Delete index entry (index path calculation)
    let index_path_str = index_path(&crate_name)?;
    let index_path = index_dir.join(&index_path_str);
    if index_path.exists() {
        if let Err(e) = std::fs::remove_file(&index_path) {
            warn!(file = %index_path.display(), error = %e, "Failed to delete index file");
        } else {
            deleted_files.push(format!("index/{crate_name}"));
        }
    }

    if deleted_files.is_empty() {
        warn!(crate_name = %crate_name, "No files found for crate");
        return Err(AppError::NotFound(format!(
            "Cargo crate '{crate_name}' not found"
        )));
    }

    info!(crate_name = %crate_name, files = ?deleted_files, "All Cargo crate versions deleted successfully");
    Ok(Json(SuccessResponse {
        message: format!(
            "Deleted {} files for Cargo crate '{}'",
            deleted_files.len(),
            crate_name
        ),
    }))
}
