use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, State},
    http::HeaderMap,
    response::Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

use crate::validation;
use crate::{
    sha1_hash, sha256_hash, storage, validate_filename, AppError, AppResult, AppState,
    SuccessResponse,
};

fn validate_package(package: &str) -> AppResult<String> {
    validation::validate_package_name(package, "npm")
        .map_err(|error| AppError::BadRequest(format!("Invalid npm package name: {error}")))
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
        validation::validate_version(version)
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
    validation::validate_package_name(&package, "npm").ok()?;
    Some(package)
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
        .join(format!("{}.json", sha256_hash(package.as_bytes())));
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

/// Downloads NPM package tarballs from local storage or upstream registry.
///
/// This endpoint serves NPM package tarballs (.tgz files) with fallback to the upstream
/// NPM registry if the file is not found locally. It supports transparent proxying
/// of packages from the official NPM registry.
///
/// # Route
/// `GET /npm/{package}/-/{filename}`
///
/// # Parameters
/// * `package` - The NPM package name
/// * `filename` - The tarball filename (e.g., "package-1.0.0.tgz")
///
/// # Returns
/// Binary tarball data as `Vec<u8>`
///
/// # Security
/// - Validates filename to prevent path traversal attacks
/// - Only serves .tgz files from the designated tarballs directory
///
/// # Example Request
/// ```text
/// GET /npm/express/-/express-4.18.2.tgz
/// ```
///
/// # Behavior
/// 1. First attempts to serve from local storage (`npm/tarballs/`)
/// 2. If not found locally, streams from upstream NPM registry
/// 3. Returns appropriate error if file not found anywhere
pub async fn download_tarball(
    AxumPath((package, filename)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    download_tarball_inner(package, filename, state).await
}

/// Downloads a scoped NPM tarball when clients normalize the encoded scope separator.
pub async fn download_scoped_tarball(
    AxumPath((scope, package, filename)): AxumPath<(String, String, String)>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    download_tarball_inner(format!("{scope}/{package}"), filename, state).await
}

async fn download_tarball_inner(
    package: String,
    filename: String,
    state: Arc<AppState>,
) -> AppResult<Vec<u8>> {
    let package = validate_package(&package)?;
    // Validate filename to prevent path traversal
    validate_filename(&filename)?;

    let local_path = state.data_dir.join("npm/tarballs").join(&filename);
    let source = state
        .resolver
        .resolve_missing(
            vm_packages::PackageEcosystem::Npm,
            &package,
            state.internal_client.is_some(),
        )
        .await?;
    let cache_scope = match source {
        vm_packages::ResolutionSource::InternalRegistry => "internal",
        vm_packages::ResolutionSource::PublicUpstream => "public",
        _ => unreachable!("cache and local releases are checked before source resolution"),
    };
    let cache_path = state
        .data_dir
        .join("cache")
        .join(cache_scope)
        .join("npm")
        .join(sha256_hash(package.as_bytes()))
        .join(&filename);
    let tarball_url = format!("/{package}/-/{filename}");
    let upstream = Arc::clone(&state.upstream_client);
    let internal = state.internal_client.clone();
    let resolved_package = package.clone();
    let resolved_filename = filename.clone();

    let data = storage::read_local_or_cache(local_path, cache_path, move || async move {
        let bytes = match source {
            vm_packages::ResolutionSource::InternalRegistry => {
                internal
                    .expect("resolver only selects a configured internal registry")
                    .npm_tarball(&resolved_package, &resolved_filename)
                    .await?
            }
            vm_packages::ResolutionSource::PublicUpstream => {
                upstream.stream_npm_tarball(&tarball_url).await?
            }
            _ => unreachable!("cache and local releases are checked before source resolution"),
        };
        Ok(bytes.to_vec())
    })
    .await?;
    debug!(
        operation = "download",
        ecosystem = "npm",
        package = %package,
        filename = %filename,
        size = data.len(),
        "package artifact served"
    );
    Ok(data)
}

/// Publishes a new NPM package version to the local registry.
///
/// This endpoint handles NPM package publishing according to the NPM registry API.
/// It processes multipart uploads containing package metadata and tarball data,
/// validates the content, and stores both the tarball and metadata.
///
/// # Route
/// `PUT /npm/{package}`
///
/// # Parameters
/// * `package` - The NPM package name to publish
/// * `payload` - JSON payload containing package metadata and base64-encoded tarball
///
/// # Payload Structure
/// ```json
/// {
///   "_id": "package-name",
///   "name": "package-name",
///   "versions": {
///     "1.0.0": {
///       "name": "package-name",
///       "version": "1.0.0",
///       "dist": {
///         "tarball": "http://server/npm/package/-/package-1.0.0.tgz"
///       }
///     }
///   },
///   "_attachments": {
///     "package-1.0.0.tgz": {
///       "data": "base64-encoded-tarball",
///       "content_type": "application/octet-stream"
///     }
///   }
/// }
/// ```
///
/// # Returns
/// JSON success response confirming package publication
///
/// # Processing Steps
/// 1. Extracts and validates `_attachments` field
/// 2. Decodes base64 tarball data
/// 3. Calculates SHA1 hash for integrity
/// 4. Saves tarball to `npm/tarballs/` directory
/// 5. Updates metadata with calculated hash
/// 6. Saves metadata to `npm/metadata/` directory
///
/// # Error Conditions
/// - Missing or invalid `_attachments` field
/// - No valid .tgz attachment found
/// - Base64 decoding failures
/// - File system write errors
pub async fn publish_package(
    AxumPath(package): AxumPath<String>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(mut payload): Json<Value>,
) -> AppResult<Json<SuccessResponse>> {
    crate::auth::validate_publish_headers(&state.config, &headers)?;
    let metadata_path = metadata_path(&state.data_dir, &package)?;
    if payload["name"].as_str() != Some(package.as_str()) {
        return Err(AppError::BadRequest(
            "npm route package and payload name must match".to_string(),
        ));
    }

    // Extract attachments containing the tarball
    let attachments = payload["_attachments"]
        .as_object()
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "Package '{package}': '_attachments' field is not an object"
            ))
        })?
        .clone();

    for (filename, attachment) in &attachments {
        if filename.ends_with(".tgz") {
            validate_filename(filename)?;

            let data_b64 = attachment["data"].as_str().ok_or_else(|| {
                AppError::UploadError("Attachment 'data' field is not a string".to_string())
            })?;

            debug!(filename = %filename, "Validating and decoding base64 tarball data");

            // Comprehensive validation before processing base64 data
            validation::validate_base64_size(data_b64, None, None).map_err(|e| {
                AppError::UploadError(format!("Base64 data size validation failed: {e}"))
            })?;

            // Validate base64 character format
            validation::validate_base64_characters(data_b64)
                .map_err(|e| AppError::UploadError(format!("Invalid base64 format: {e}")))?;

            // Decode base64 tarball with comprehensive error handling
            let tarball_data = general_purpose::STANDARD
                .decode(data_b64)
                .map_err(|e| AppError::UploadError(format!("Invalid base64 encoding: {e}")))?;

            // Use centralized validation for decoded tarball size
            validation::validate_package_upload(&tarball_data, filename, "NPM")?;

            // Calculate SHA1 hash for metadata
            let shasum = sha1_hash(&tarball_data);

            // Update metadata with correct tarball URL and hash
            if let Some(versions) = payload["versions"].as_object_mut() {
                for version_data in versions.values_mut() {
                    if let Some(dist) = version_data.get_mut("dist").and_then(|d| d.as_object_mut())
                    {
                        dist.insert("shasum".to_string(), json!(shasum));
                    }
                }
            }

            // Remove attachments before saving metadata
            if let Some(obj) = payload.as_object_mut() {
                obj.remove("_attachments");
            }

            let _publish_guard = storage::publish_guard().await;
            let merged = merge_metadata(&metadata_path, &payload).await?;

            // Commit the immutable artifact before its discoverable metadata.
            let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
            storage::save_immutable(tarball_path, &tarball_data).await?;

            // Save metadata
            let metadata_str = serde_json::to_string_pretty(&merged)?;
            storage::save_file(metadata_path, metadata_str.as_bytes()).await?;

            info!(
                operation = "publish",
                ecosystem = "npm",
                package = %package,
                filename = %filename,
                size = tarball_data.len(),
                outcome = "published",
                "package publication completed"
            );
            return Ok(Json(SuccessResponse {
                message: "Package published successfully".to_string(),
            }));
        }
    }

    Err(AppError::UploadError(
        "No valid .tgz attachment found".to_string(),
    ))
}

/// Deletes a specific version of an NPM package from the registry.
///
/// This endpoint removes a single version of an NPM package, updating the metadata
/// to remove the version entry and deleting the associated tarball file. This is
/// equivalent to NPM's unpublish functionality for specific versions.
///
/// # Route
/// `DELETE /npm/{package_name}/{version}`
///
/// # Parameters
/// * `package_name` - The NPM package name
/// * `version` - The specific version to delete (e.g., "1.0.0")
///
/// # Returns
/// JSON response confirming successful deletion
///
/// # Example Response
/// ```json
/// {
///   "message": "Unpublished version 1.0.0 of NPM package 'my-package'"
/// }
/// ```
///
/// # Processing Steps
/// 1. Updates package metadata JSON to remove the specified version
/// 2. Deletes the corresponding tarball file (`package-version.tgz`)
/// 3. Logs deletion for audit purposes
///
/// # Error Conditions
/// - Package or version not found
/// - File system access errors
/// - JSON parsing/writing errors
///
/// # Note
/// If this is the last version of a package, the metadata file will still exist
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, UpstreamClient};
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use base64::engine::general_purpose;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn create_npm_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("npm/tarballs"))
            .expect("Failed to create npm tarballs dir");
        std::fs::create_dir_all(data_dir.join("npm/metadata"))
            .expect("Failed to create npm metadata dir");

        let config = Arc::new(crate::config::Config::default());
        let state = Arc::new(AppState {
            data_dir,
            server_addr: "http://localhost:8080".to_string(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            internal_client: None,
            config,
            resolver: Arc::new(crate::resolver::ResolverService::standalone()),
        });

        (state, temp_dir)
    }

    fn create_npm_publish_payload(
        package_name: &str,
        version: &str,
        tarball_content: &[u8],
    ) -> Value {
        let encoded_tarball = general_purpose::STANDARD.encode(tarball_content);
        let filename = format!("{}-{}.tgz", package_name, version);

        json!({
            "_id": package_name,
            "name": package_name,
            "description": "Test package",
            "dist-tags": {
                "latest": version
            },
            "versions": {
                version: {
                    "name": package_name,
                    "version": version,
                    "description": "Test package",
                    "dist": {
                        "tarball": format!("http://localhost:8080/npm/{}/-/{}", package_name, filename)
                    }
                }
            },
            "_attachments": {
                filename: {
                    "content_type": "application/octet-stream",
                    "data": encoded_tarball,
                    "length": tarball_content.len()
                }
            }
        })
    }

    #[test]
    fn metadata_paths_encode_scopes_and_reject_traversal() {
        let root = std::path::Path::new("/registry");
        assert_eq!(
            metadata_path(root, "@scope/package").unwrap(),
            root.join("npm/metadata/@scope%2Fpackage.json")
        );
        assert_eq!(
            package_from_metadata_file_name("@scope%2Fpackage.json").as_deref(),
            Some("@scope/package")
        );
        for package in ["..", "../outside", "@scope/../outside", "/tmp/outside"] {
            assert!(metadata_path(root, package).is_err());
        }
    }

    #[tokio::test]
    async fn test_publish_package_with_tarball() {
        let (state, _temp_dir) = create_npm_test_state();
        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::put(publish_package))
            .with_state(state.clone());

        let server = TestServer::new(app);

        let package_name = "test-package";
        let version = "1.0.0";
        let tarball_content = b"fake tarball content";
        let payload = create_npm_publish_payload(package_name, version, tarball_content);

        let response = server
            .put(&format!("/npm/{}", package_name))
            .json(&payload)
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify tarball was saved
        let tarball_path = state
            .data_dir
            .join("npm/tarballs")
            .join(format!("{}-{}.tgz", package_name, version));
        assert!(tarball_path.exists());
        let saved_content = std::fs::read(tarball_path).expect("should read saved tarball");
        assert_eq!(saved_content, tarball_content);

        // Verify metadata was saved
        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{}.json", package_name));
        assert!(metadata_path.exists());
        let metadata_content =
            std::fs::read_to_string(metadata_path).expect("should read saved metadata");
        let metadata: Value =
            serde_json::from_str(&metadata_content).expect("should parse metadata");

        // Verify _attachments was removed from saved metadata
        assert!(metadata.get("_attachments").is_none());

        // Verify shasum was calculated and added
        assert!(metadata["versions"][version]["dist"]["shasum"].is_string());
    }

    #[tokio::test]
    async fn test_publish_package_rejects_no_attachments() {
        let (state, _temp_dir) = create_npm_test_state();
        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::put(publish_package))
            .with_state(state);

        let server = TestServer::new(app);

        let payload = json!({
            "name": "test-package",
            "version": "1.0.0"
        });

        let response = server.put("/npm/test-package").json(&payload).await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_publish_package_rejects_path_attachment_name() {
        let (state, _temp_dir) = create_npm_test_state();
        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::put(publish_package))
            .with_state(state);

        let server = TestServer::new(app);

        let tarball_content = b"fake tarball content";
        let encoded_tarball = general_purpose::STANDARD.encode(tarball_content);
        let payload = json!({
            "name": "test-package",
            "versions": {
                "1.0.0": {
                    "name": "test-package",
                    "version": "1.0.0",
                    "dist": {
                        "tarball": "http://localhost:8080/npm/test-package/-/test-package-1.0.0.tgz"
                    }
                }
            },
            "_attachments": {
                "../test-package-1.0.0.tgz": {
                    "content_type": "application/octet-stream",
                    "data": encoded_tarball,
                    "length": tarball_content.len()
                }
            }
        });

        let response = server.put("/npm/test-package").json(&payload).await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_package_metadata_after_publish() {
        let (state, _temp_dir) = create_npm_test_state();

        // Create test metadata file
        let package_name = "test-package";
        let metadata = json!({
            "name": package_name,
            "dist-tags": { "latest": "1.0.0" },
            "versions": {
                "1.0.0": {
                    "name": package_name,
                    "version": "1.0.0",
                    "dist": {
                        "tarball": "http://localhost:8080/npm/test-package/-/test-package-1.0.0.tgz",
                        "shasum": "abc123"
                    }
                }
            }
        });

        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{}.json", package_name));
        std::fs::write(
            metadata_path,
            serde_json::to_string_pretty(&metadata).expect("should serialize metadata"),
        )
        .expect("should write metadata file");

        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::get(package_metadata))
            .with_state(state);

        let server = TestServer::new(app);
        let response = server.get(&format!("/npm/{}", package_name)).await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body: Value = response.json();
        assert_eq!(body["name"], package_name);
        assert_eq!(body["dist-tags"]["latest"], "1.0.0");
        assert!(body["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .expect("tarball URL should be a string")
            .contains("test-package-1.0.0.tgz"));
    }

    #[tokio::test]
    async fn test_package_metadata_updates_host_header() {
        let (state, _temp_dir) = create_npm_test_state();

        // Create test metadata file with localhost URL
        let package_name = "test-package";
        let metadata = json!({
            "name": package_name,
            "versions": {
                "1.0.0": {
                    "dist": {
                        "tarball": "http://localhost:8080/npm/test-package/-/test-package-1.0.0.tgz"
                    }
                }
            }
        });

        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{}.json", package_name));
        std::fs::write(
            metadata_path,
            serde_json::to_string_pretty(&metadata).expect("should serialize metadata"),
        )
        .expect("should write metadata file");

        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::get(package_metadata))
            .with_state(state);

        let server = TestServer::new(app);
        let response = server
            .get(&format!("/npm/{}", package_name))
            .add_header("host", "example.com:3000")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body: Value = response.json();

        // Reverse-proxied package URLs use the validated public request authority.
        let tarball_url = body["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .expect("tarball URL should be a string");
        assert!(tarball_url.contains("example.com:3000"));
    }

    #[tokio::test]
    async fn test_download_tarball() {
        let (state, _temp_dir) = create_npm_test_state();

        // Create test tarball file
        let content = b"test tarball content";
        let filename = "test-package-1.0.0.tgz";
        let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
        std::fs::write(&tarball_path, content).expect("should write test tarball");

        let app = axum::Router::new()
            .route(
                "/npm/{package}/-/{filename}",
                axum::routing::get(download_tarball),
            )
            .with_state(state);

        let server = TestServer::new(app);
        let response = server
            .get("/npm/test-package/-/test-package-1.0.0.tgz")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.as_bytes().to_vec(), content.to_vec());
    }

    #[tokio::test]
    async fn test_download_scoped_tarball_with_decoded_separator() {
        let (state, _temp_dir) = create_npm_test_state();
        let content = b"scoped tarball content";
        let filename = "fs-minipass-4.0.1.tgz";
        let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
        std::fs::write(&tarball_path, content).expect("should write test tarball");

        let app = axum::Router::new()
            .route(
                "/npm/{scope}/{package}/-/{filename}",
                axum::routing::get(download_scoped_tarball),
            )
            .with_state(state);

        let response = TestServer::new(app)
            .get("/npm/@isaacs/fs-minipass/-/fs-minipass-4.0.1.tgz")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.as_bytes().to_vec(), content.to_vec());
    }
}
