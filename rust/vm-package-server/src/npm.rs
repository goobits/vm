use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, State},
    http::HeaderMap,
    response::Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use vm_packages::{encode_hex, sha256_hex, PackageEcosystem, PackageIdentity};

mod download;
mod publish;
pub use download::{download_scoped_tarball, download_tarball};
pub use publish::publish_package;

use crate::validation;
use crate::{storage, AppError, AppResult, AppState, SuccessResponse};

fn sha1_hex(data: &[u8]) -> String {
    encode_hex(Sha1::digest(data))
}

fn validate_package(package: &str) -> AppResult<String> {
    let identity = PackageIdentity::new(PackageEcosystem::Npm, package)
        .map_err(|error| AppError::BadRequest(format!("Invalid npm package name: {error}")))?;
    if identity.name != package {
        return Err(AppError::BadRequest(
            "Invalid npm package name: npm package names must be lowercase".into(),
        ));
    }
    Ok(identity.name)
}

async fn merge_metadata(path: &Path, incoming: &Value) -> AppResult<Value> {
    let incoming_versions = incoming["versions"].as_object().ok_or_else(|| {
        AppError::BadRequest("npm publish payload must contain versions".to_string())
    })?;
    if incoming_versions.is_empty() {
        return Err(AppError::BadRequest(
            "npm publish payload must contain one version".to_string(),
        ));
    }
    for version in incoming_versions.keys() {
        validation::validate_registry_version(version)
            .map_err(|error| AppError::BadRequest(format!("Invalid npm version: {error}")))?;
    }

    let existing = match storage::read_file_string(path).await {
        Ok(existing) => existing,
        Err(AppError::NotFound(_)) => return Ok(incoming.clone()),
        Err(error) => return Err(error),
    };
    let mut merged: Value = serde_json::from_str(&existing)?;
    let versions = merged["versions"].as_object_mut().ok_or_else(|| {
        AppError::InternalError("stored npm metadata has no versions object".to_string())
    })?;
    for (version, metadata) in incoming_versions {
        match versions.get(version) {
            Some(existing) if existing == metadata => {}
            Some(_) => {
                return Err(AppError::Conflict(format!(
                    "npm package version '{version}' is already published"
                )))
            }
            None => {
                versions.insert(version.clone(), metadata.clone());
            }
        }
    }
    if let Some(incoming_tags) = incoming["dist-tags"].as_object() {
        let tags = merged["dist-tags"].as_object_mut().ok_or_else(|| {
            AppError::InternalError("stored npm metadata has no dist-tags object".to_string())
        })?;
        tags.extend(incoming_tags.clone());
    }
    Ok(merged)
}

fn metadata_file_name(package: &str) -> String {
    format!("{}.json", package.replace('/', "%2F"))
}

pub(crate) fn package_from_metadata_file_name(file_name: &str) -> Option<String> {
    let encoded = file_name.strip_suffix(".json")?;
    let package = encoded.replace("%2F", "/").replace("%2f", "/");
    validate_package(&package).ok()
}

pub(crate) fn metadata_path(data_dir: &Path, package: &str) -> AppResult<PathBuf> {
    let package = validate_package(package)?;
    let file_name = metadata_file_name(&package);
    Ok(data_dir.join("npm/metadata").join(file_name))
}

/// Returns NPM package metadata including all versions and download information.
///
/// Serves package metadata compatible with NPM registry API, including version information,
/// dependencies, and download URLs. Falls back to upstream NPM registry if package
/// is not found locally.
///
/// # Route
/// `GET /npm/{package}`
///
/// # Parameters
/// * `package` - The NPM package name (supports scoped packages like @scope/package)
///
/// # Returns
/// JSON object containing complete package metadata
///
/// # Example Response
/// ```json
/// {
///   "name": "package-name",
///   "versions": {
///     "1.0.0": {
///       "name": "package-name",
///       "version": "1.0.0",
///       "dist": {
///         "tarball": "http://localhost:8080/npm/package-name/-/package-name-1.0.0.tgz",
///         "shasum": "abcd1234..."
///       }
///     }
///   }
/// }
/// ```
pub async fn package_metadata(
    AxumPath(package): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> AppResult<Json<Value>> {
    let host = state.public_base_url(&headers);
    let metadata_path = metadata_path(&state.data_dir, &package)?;

    // Check if metadata file exists
    match storage::read_file_string(&metadata_path).await {
        Ok(content) => {
            let mut metadata = serde_json::from_str::<Value>(&content)?;
            if let Some(versions) = metadata["versions"].as_object_mut() {
                for version_data in versions.values_mut() {
                    if let Some(dist) = version_data["dist"].as_object_mut() {
                        if let Some(tarball) = dist["tarball"].as_str() {
                            if let Some(path) = tarball.split("/npm/").nth(1) {
                                dist["tarball"] = json!(format!("{host}/npm/{path}"));
                            }
                        }
                    }
                }
            }
            return Ok(Json(metadata));
        }
        Err(AppError::NotFound(_)) => {}
        Err(error) => return Err(error),
    }

    // No local metadata found, try upstream NPM
    let source = state
        .resolver
        .resolve_missing(
            vm_packages::PackageEcosystem::Npm,
            &package,
            state.internal_client.is_some(),
        )
        .await?;
    debug!(package = %package, source = ?source, "No local metadata found, resolving npm source");
    let cache_scope = match source {
        vm_packages::ResolutionSource::InternalRegistry => "internal",
        vm_packages::ResolutionSource::PublicUpstream => "public",
        _ => unreachable!("local releases are checked before source resolution"),
    };
    let cache_path = state
        .data_dir
        .join("cache")
        .join(cache_scope)
        .join("npm/metadata")
        .join(format!("{}.json", sha256_hex(package.as_bytes())));
    let upstream = Arc::clone(&state.upstream_client);
    let internal = state.internal_client.clone();
    let resolved_package = package.clone();
    let metadata = storage::read_refreshing_cache(
        cache_path,
        storage::METADATA_CACHE_TTL,
        move || async move {
            let metadata = match source {
                vm_packages::ResolutionSource::InternalRegistry => {
                    internal
                        .expect("resolver only selects a configured internal registry")
                        .npm_metadata(&resolved_package)
                        .await?
                }
                vm_packages::ResolutionSource::PublicUpstream => {
                    upstream.fetch_npm_metadata(&resolved_package).await?
                }
                _ => unreachable!("local releases are checked before source resolution"),
            };
            serde_json::to_vec(&metadata).map_err(AppError::from)
        },
    )
    .await?;
    let metadata = serde_json::from_slice(&metadata)?;
    debug!(
        operation = "resolve_metadata",
        ecosystem = "npm",
        package = %package,
        source = ?source,
        "package metadata resolved"
    );
    Ok(Json(
        state
            .upstream_client
            .update_npm_tarball_urls(metadata, &host, &package),
    ))
}

#[cfg(test)]
mod tests;
