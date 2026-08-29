//! Cargo HTTP endpoint handlers
//!
//! This module contains all HTTP endpoint handlers for the Cargo registry,
//! including package uploads, downloads, version management, and configuration.

use super::{index::*, parsing::*, storage::*};
use crate::{package_utils, storage, validation, AppError, AppResult, AppState, SuccessResponse};
use axum::{
    extract::{Path as AxumPath, State},
    http::HeaderMap,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};
use vm_packages::sha256_hex;

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
                        warn!(
                            operation = "read_index",
                            ecosystem = "cargo",
                            filename = %filename,
                            error = %e,
                            "invalid persisted package filename skipped"
                        );
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
    debug!(
        operation = "list_versions",
        ecosystem = "cargo",
        package = %crate_name,
        "package versions requested"
    );
    let versions_data = get_crate_versions(&state, &crate_name).await?;

    // Extract just the version strings
    let versions: Vec<String> = versions_data
        .into_iter()
        .map(|(version, _, _)| version)
        .collect();

    Ok(Json(versions))
}

/// Get recent crates
/// Returns Cargo registry configuration required for client setup.
pub async fn config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let host = state.public_base_url(&headers);

    Ok(Json(json!({
        "dl": format!("{}/cargo/api/v1/crates/{{crate}}/{{version}}/download", host),
        "api": format!("{}/cargo", host),
        "auth-required": state.config.security.require_authentication
    })))
}

/// Downloads Cargo crate files from local storage or upstream registry.
pub async fn download_crate(
    AxumPath((crate_name, version)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    // Validate crate name and version for security
    super::validate_crate_name(&crate_name)?;
    validation::validate_registry_version(&version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version '{version}': {e}")))?;

    let filename = format!("{crate_name}-{version}.crate");

    // Validate the constructed filename for security
    validation::validate_safe_path(&filename).map_err(|e| {
        AppError::BadRequest(format!("Generated unsafe filename '{filename}': {e}"))
    })?;

    let local_path = state.data_dir.join("cargo/crates").join(&filename);
    let fallback_state = Arc::clone(&state);
    let fallback_crate = crate_name.clone();
    let fallback_version = version.clone();
    let data = storage::read_local_or_else(local_path, move || async move {
        let source = fallback_state
            .resolver
            .resolve_missing(
                vm_packages::PackageEcosystem::Cargo,
                &fallback_crate,
                fallback_state.internal_client.is_some(),
            )
            .await?;
        let cache_scope = match source {
            vm_packages::ResolutionSource::InternalRegistry => "internal",
            vm_packages::ResolutionSource::PublicUpstream => "public",
            _ => unreachable!("local releases are checked before source resolution"),
        };
        let cache_path = fallback_state
            .data_dir
            .join("cache")
            .join(cache_scope)
            .join("cargo/crates")
            .join(format!("{fallback_crate}-{fallback_version}.crate"));
        storage::read_through_cache(cache_path, move || async move {
            let bytes = match source {
                vm_packages::ResolutionSource::InternalRegistry => {
                    fallback_state
                        .internal_client
                        .as_ref()
                        .expect("resolver only selects a configured internal registry")
                        .cargo_crate(&fallback_crate, &fallback_version)
                        .await?
                }
                vm_packages::ResolutionSource::PublicUpstream => {
                    fallback_state
                        .upstream_client
                        .stream_cargo_crate(&fallback_crate, &fallback_version)
                        .await?
                }
                _ => unreachable!("local releases are checked before source resolution"),
            };
            Ok(bytes.to_vec())
        })
        .await
    })
    .await?;
    debug!(
        operation = "download",
        ecosystem = "cargo",
        package = %crate_name,
        version = %version,
        size = data.len(),
        "package artifact served"
    );
    Ok(data)
}

/// Publishes a new Cargo crate version to the local registry.
pub async fn publish_crate(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> AppResult<Json<SuccessResponse>> {
    crate::auth::validate_publish_headers(&state.config, &headers)?;

    // Parse and validate the upload payload
    let (metadata, crate_data) = parse_crate_upload(body)?;
    let _publish_guard = storage::publish_guard().await;

    // Save the crate file
    save_crate_file(
        &crate_data,
        &metadata.name,
        &metadata.version,
        &state.data_dir,
    )
    .await?;

    // Calculate checksum
    let cksum = sha256_hex(&crate_data);

    // Update the index
    update_crate_index(&metadata, &cksum, &state.data_dir).await?;

    info!(
        operation = "publish",
        ecosystem = "cargo",
        package = %metadata.name,
        version = %metadata.version,
        artifact_digest = %cksum,
        size = crate_data.len(),
        outcome = "published",
        "package publication completed"
    );
    Ok(Json(SuccessResponse {
        message: "Crate published successfully".to_string(),
    }))
}
