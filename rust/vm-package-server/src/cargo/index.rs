//! Cargo index management operations
//!
//! This module handles Cargo registry index operations including path calculation,
//! index file management, and sparse index serving.

use crate::{storage, validation, AppError, AppResult, AppState};
use axum::{
    extract::{Path as AxumPath, State},
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::Arc;
use tracing::debug;

/// Calculate Cargo index path for a crate name according to Cargo's index structure
/// Names are organized in directories: 1/a, 2/ab, 3/a/abc, ab/cd/abcd...
///
/// # Security
/// This function validates the crate name and performs bounds checking to prevent
/// directory traversal attacks and panics from malformed input.
pub fn index_path(name: &str) -> AppResult<String> {
    // Validate the crate name first
    let validated_name = super::validate_crate_name(name)?;

    let name = validated_name.to_lowercase();

    // Perform bounds checking for string slicing operations
    let path = match name.len() {
        0 => {
            return Err(AppError::BadRequest(
                "Crate name cannot be empty".to_string(),
            ))
        }
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => {
            // Safe: we know name.len() == 3, so [..1] is valid
            let first_char = &name[..1];
            format!("3/{first_char}/{name}")
        }
        _ => {
            // Safe bounds checking for longer names
            if name.len() < 4 {
                return Err(AppError::BadRequest(format!(
                    "Invalid crate name length: {}",
                    name.len()
                )));
            }
            let first_two = &name[..2];
            let next_two = &name[2..4];
            format!("{first_two}/{next_two}/{name}")
        }
    };

    // Validate the constructed path for safety
    validation::validate_safe_path(&path)
        .map_err(|e| AppError::BadRequest(format!("Generated unsafe index path '{path}': {e}")))?;

    Ok(path)
}

/// Update the crate index with new crate information
pub async fn update_crate_index(
    metadata: &super::CrateMetadata,
    checksum: &str,
    data_dir: &std::path::Path,
) -> AppResult<()> {
    // Create index entry
    let index_entry = json!({
        "name": metadata.name,
        "vers": metadata.version,
        "deps": metadata.deps,
        "cksum": checksum,
        "features": metadata.features,
        "yanked": false
    });

    // Update index file
    let index_path_str = index_path(&metadata.name)?;
    let index_file_path = data_dir.join("cargo/index").join(&index_path_str);
    debug!(index_path = %index_file_path.display(), "Updating Cargo index");

    if let Ok(existing) = storage::read_file_string(&index_file_path).await {
        for entry in existing
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        {
            if entry["vers"].as_str() == Some(metadata.version.as_str()) {
                if entry["cksum"].as_str() == Some(checksum) {
                    return Ok(());
                }
                return Err(AppError::Conflict(format!(
                    "crate '{}@{}' is already published",
                    metadata.name, metadata.version
                )));
            }
        }
    }

    // Append to index file using storage utility
    let index_line = serde_json::to_string(&index_entry)?;
    storage::append_to_file(index_file_path, &index_line).await?;

    Ok(())
}

/// Serves Cargo index files containing crate version metadata.
///
/// This endpoint serves index files according to Cargo's index structure, returning
/// newline-delimited JSON containing metadata for all versions of a crate. Falls back
/// to upstream crates.io if the crate is not found locally.
pub async fn index_file(
    axum::extract::Path(path): axum::extract::Path<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<String> {
    // Extract crate name from the path (e.g., "2/ab" or "3/a/bc" or "ab/cd/abcd")
    let crate_name = path
        .split('/')
        .next_back()
        .ok_or_else(|| AppError::BadRequest(format!("Cargo index path format is invalid: '{path}' - expected format: 1/a, 2/ab, 3/a/abc, or ab/cd/abcd...")))?;

    // Validate the extracted crate name for security
    super::validate_crate_name(crate_name).map_err(|error| {
        AppError::BadRequest(format!(
            "Invalid crate name '{crate_name}' extracted from path '{path}': {error}"
        ))
    })?;

    debug!(crate_name = %crate_name, path = %path, "resolving Cargo index request");
    let index_path_str = index_path(crate_name)?;
    let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);

    // Try local index first
    match storage::read_file_string(&index_file_path).await {
        Ok(content) => {
            debug!(crate_name = %crate_name, "Serving index from local storage");
            Ok(content)
        }
        Err(AppError::NotFound(_)) => {
            // Index not found locally, try upstream crates.io
            let source = state
                .resolver
                .resolve_missing(
                    vm_packages::PackageEcosystem::Cargo,
                    crate_name,
                    state.internal_client.is_some(),
                )
                .await?;
            debug!(crate_name = %crate_name, source = ?source, "Index not found locally, resolving Cargo source");
            let cache_scope = match source {
                vm_packages::ResolutionSource::InternalRegistry => "internal",
                vm_packages::ResolutionSource::PublicUpstream => "public",
                _ => unreachable!("local releases are checked before source resolution"),
            };
            let cache_path = state
                .data_dir
                .join("cache")
                .join(cache_scope)
                .join("cargo/index")
                .join(&index_path_str);
            let internal = state.internal_client.clone();
            let upstream = Arc::clone(&state.upstream_client);
            let resolved_path = path.clone();
            let resolved_crate = crate_name.to_string();
            let content = storage::read_refreshing_cache(
                cache_path,
                storage::METADATA_CACHE_TTL,
                move || async move {
                    let content = match source {
                        vm_packages::ResolutionSource::InternalRegistry => {
                            internal
                                .expect("resolver only selects a configured internal registry")
                                .cargo_index(&resolved_path)
                                .await?
                        }
                        vm_packages::ResolutionSource::PublicUpstream => {
                            upstream
                                .fetch_cargo_index(&resolved_crate, &resolved_path)
                                .await?
                        }
                        _ => unreachable!("local releases are checked before source resolution"),
                    };
                    Ok(content.into_bytes())
                },
            )
            .await?;
            String::from_utf8(content).map_err(|error| AppError::Utf8(error.utf8_error()))
        }
        Err(error) => Err(error),
    }
}

/// Sparse index handler for Cargo registries
///
/// This serves the sparse index format which is HTTP-based instead of Git-based.
/// For sparse indexes, Cargo requests individual crate metadata files directly
/// via HTTP rather than cloning a Git repository.
pub async fn sparse_index(
    AxumPath(path): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<Response> {
    // Handle config.json separately
    if path == "config.json" {
        return super::config(State(state), headers)
            .await
            .map(|json| json.into_response());
    }

    // For sparse index, the path format is different:
    // - 2-char prefix: /ca/rg/cargo
    // - 3-char: /3/c/cargo
    // - 4+ char: first 2 chars / next 2 chars / crate_name

    // Extract the crate name from the path
    let parts: Vec<&str> = path.split('/').collect();
    let crate_name = parts
        .last()
        .ok_or_else(|| AppError::BadRequest(format!("Invalid sparse index path: {path}")))?;

    debug!(crate_name = %crate_name, path = %path, "Sparse index request");

    // Use the existing index_file logic
    index_file(AxumPath(path), State(state))
        .await
        .map(|content| content.into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_path_algorithm() {
        assert_eq!(index_path("a").expect("should get path for 'a'"), "1/a");
        assert_eq!(index_path("ab").expect("should get path for 'ab'"), "2/ab");
        assert_eq!(
            index_path("abc").expect("should get path for 'abc'"),
            "3/a/abc"
        );
        assert_eq!(
            index_path("abcd").expect("should get path for 'abcd'"),
            "ab/cd/abcd"
        );
        assert_eq!(
            index_path("serde").expect("should get path for 'serde'"),
            "se/rd/serde"
        );
        assert_eq!(
            index_path("SERDE").expect("should get path for 'SERDE'"),
            "se/rd/serde"
        ); // test lowercase conversion

        // Test security: invalid characters should be rejected
        assert!(index_path("../malicious").is_err());
        assert!(index_path("crate/../etc/passwd").is_err());
        assert!(index_path("/absolute/path").is_err());
        assert!(index_path("crate with spaces").is_err());
        assert!(index_path("").is_err());
    }
}
