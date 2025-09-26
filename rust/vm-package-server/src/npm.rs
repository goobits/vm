use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, State},
    response::Json,
};
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::validation;
use vm_package_server::validation_utils::FileStreamValidator;
use vm_package_server::{
    package_utils, sha1_hash, storage, validate_filename, AppError, AppResult, AppState,
    SuccessResponse,
};

/// Counts the total number of unique NPM packages in the repository.
///
/// Scans the NPM metadata directory for .json files and returns the count of unique packages.
/// Each .json file represents one package's metadata.
///
/// # Arguments
/// * `state` - Application state containing the data directory path
///
/// # Returns
/// The number of unique NPM packages stored in the repository
///
/// # Example
/// ```rust
/// let count = count_packages(&app_state).await?;
/// println!("Repository contains {} NPM packages", count);
/// ```
#[allow(dead_code)]
pub async fn count_packages(state: &AppState) -> AppResult<usize> {
    package_utils::count_packages_by_pattern(
        &state.data_dir,
        &package_utils::RegistryPattern::NPM,
        |filename| {
            // Extract package name from filename (remove .json extension)
            filename.strip_suffix(".json").map(|name| name.to_string())
        },
    )
    .await
}

/// Returns a sorted list of all NPM package names in the repository.
///
/// Scans the NPM metadata directory for .json files, extracts package names,
/// and returns them in alphabetical order.
///
/// # Arguments
/// * `state` - Application state containing the data directory path
///
/// # Returns
/// A vector of package names sorted alphabetically
///
/// # Example
/// ```rust
/// let packages = list_all_packages(&app_state).await?;
/// for package in packages {
///     println!("NPM Package: {}", package);
/// }
/// ```
#[allow(dead_code)]
pub async fn list_all_packages(state: &AppState) -> AppResult<Vec<String>> {
    package_utils::list_packages_by_pattern(
        &state.data_dir,
        &package_utils::RegistryPattern::NPM,
        |filename| {
            // Extract package name from filename (remove .json extension)
            filename.strip_suffix(".json").map(|name| name.to_string())
        },
    )
    .await
}

/// Get package versions with tarball info and file sizes
pub async fn get_package_versions(
    state: &AppState,
    package_name: &str,
) -> AppResult<Vec<(String, String, String, u64)>> {
    let metadata_path = state
        .data_dir
        .join("npm/metadata")
        .join(format!("{}.json", package_name));
    let mut versions = Vec::new();

    let result = storage::read_file_string(&metadata_path).await;
    if let Ok(content) = result {
        if let Ok(metadata) = serde_json::from_str::<Value>(&content) {
            if let Some(versions_obj) = metadata["versions"].as_object() {
                for (version, data) in versions_obj {
                    if let Some(dist) = data["dist"].as_object() {
                        let tarball = dist
                            .get("tarball")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let shasum = dist
                            .get("shasum")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        // Extract filename from tarball URL and get file size
                        let filename = tarball.split('/').next_back().unwrap_or("");
                        let file_path = state.data_dir.join("npm/packages").join(filename);
                        let size = package_utils::get_file_size(&file_path).await;

                        versions.push((version.clone(), tarball, shasum, size));
                    }
                }
            }
        }
    }

    versions.sort_by(|a, b| b.0.cmp(&a.0)); // Sort versions in reverse
    Ok(versions)
}

/// Get recent packages
pub async fn get_recent_packages(
    state: &AppState,
    limit: usize,
) -> AppResult<Vec<(String, String)>> {
    // Get recent metadata files using pattern helper, then read JSON to extract latest versions
    let recent_files = package_utils::get_recent_packages_by_pattern(
        &state.data_dir,
        &package_utils::RegistryPattern::NPM,
        limit,
        |filename| {
            // Extract package name from filename (remove .json extension)
            filename
                .strip_suffix(".json")
                .map(|name| (name.to_string(), String::new()))
        },
    )
    .await?;

    let mut recent = Vec::new();
    for (package_name, _) in recent_files {
        // Read metadata file to get latest version
        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{}.json", package_name));
        if let Ok(content) = storage::read_file_string(&metadata_path).await {
            if let Ok(metadata) = serde_json::from_str::<Value>(&content) {
                if let Some(latest) = metadata["dist-tags"]["latest"].as_str() {
                    recent.push((package_name, latest.to_string()));
                }
            }
        }
    }

    Ok(recent)
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
) -> AppResult<Json<Value>> {
    debug!(package = %package, "Incoming npm metadata request");
    let host = &state.server_addr;
    let metadata_path = state
        .data_dir
        .join("npm/metadata")
        .join(format!("{}.json", package));

    info!(package = %package, "Fetching npm package metadata");

    // Check if metadata file exists
    let result = storage::read_file_string(&metadata_path).await;
    if let Ok(content) = result {
        if let Ok(mut metadata) = serde_json::from_str::<Value>(&content) {
            // Update tarball URLs with current host
            if let Some(versions) = metadata["versions"].as_object_mut() {
                for version_data in versions.values_mut() {
                    if let Some(dist) = version_data["dist"].as_object_mut() {
                        if let Some(tarball) = dist["tarball"].as_str() {
                            // Replace the host in the tarball URL
                            if let Some(path) = tarball.split("/npm/").nth(1) {
                                dist["tarball"] = json!(format!("{}/npm/{}", host, path));
                            }
                        }
                    }
                }
            }
            return Ok(Json(metadata));
        }
    }

    // No local metadata found, try upstream NPM
    debug!(package = %package, "No local metadata found, checking upstream NPM");
    match state.upstream_client.fetch_npm_metadata(&package).await {
        Ok(upstream_metadata) => {
            info!(package = %package, "Found package on upstream NPM, updating URLs and returning");
            // Update tarball URLs to point to our server for transparent proxying
            let updated_metadata = state
                .upstream_client
                .update_npm_tarball_urls(upstream_metadata, host);
            Ok(Json(updated_metadata))
        }
        Err(_) => {
            debug!(package = %package, "Package not found on upstream NPM either");
            // Return 404 as per npm registry behavior
            Err(AppError::NotFound(format!(
                "Package not found: {}",
                package
            )))
        }
    }
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
/// ```
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
    // Validate filename to prevent path traversal
    validate_filename(&filename)?;

    debug!(package = %package, filename = %filename, "Incoming npm tarball download request");
    info!(package = %package, filename = %filename, "Downloading npm tarball");
    let file_path = state.data_dir.join("npm/tarballs").join(&filename);

    // Try local file first
    match storage::read_file(&file_path).await {
        Ok(data) => {
            debug!(package = %package, filename = %filename, size = data.len(), "Serving tarball from local storage");
            Ok(data)
        }
        Err(_) => {
            // File not found locally, try upstream NPM
            debug!(package = %package, filename = %filename, "Tarball not found locally, checking upstream NPM");

            // Construct the tarball URL for upstream
            let tarball_url = format!("/{package}/-/{filename}");
            match state.upstream_client.stream_npm_tarball(&tarball_url).await {
                Ok(bytes) => {
                    info!(package = %package, filename = %filename, size = bytes.len(), "Streaming tarball from upstream NPM");
                    Ok(bytes.to_vec())
                }
                Err(e) => {
                    debug!(package = %package, filename = %filename, error = %e, "Tarball not found on upstream NPM either");
                    Err(e)
                }
            }
        }
    }
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
    Json(mut payload): Json<Value>,
) -> AppResult<Json<SuccessResponse>> {
    debug!(package = %package, "Incoming npm publish request");
    info!(package = %package, "Publishing npm package");

    // Extract attachments containing the tarball
    let attachments = payload["_attachments"]
        .as_object()
        .ok_or_else(|| {
            warn!(package = %package, "npm publish payload _attachments field is not an object");
            AppError::BadRequest(format!(
                "Package '{}': '_attachments' field is not an object",
                package
            ))
        })?
        .clone();

    for (filename, attachment) in &attachments {
        if filename.ends_with(".tgz") {
            let data_b64 = attachment["data"].as_str()
                .ok_or_else(|| {
                    warn!(package = %package, filename = %filename, "npm attachment data field is not a string");
                    AppError::UploadError("Attachment 'data' field is not a string".to_string())
                })?;

            debug!(filename = %filename, "Validating and decoding base64 tarball data");

            // Comprehensive validation before processing base64 data
            validation::validate_base64_size(data_b64, None, None).map_err(|e| {
                warn!(package = %package, filename = %filename, error = %e,
                      "Base64 size validation failed");
                AppError::UploadError(format!("Base64 data size validation failed: {}", e))
            })?;

            // Validate base64 character format
            validation::validate_base64_characters(data_b64).map_err(|e| {
                warn!(package = %package, filename = %filename, error = %e,
                      "Base64 character validation failed");
                AppError::UploadError(format!("Invalid base64 format: {}", e))
            })?;

            // Decode base64 tarball with comprehensive error handling
            let tarball_data = general_purpose::STANDARD.decode(data_b64).map_err(|e| {
                warn!(package = %package, filename = %filename, error = %e,
                      "Failed to decode base64 data");
                AppError::UploadError(format!("Invalid base64 encoding: {}", e))
            })?;

            // Use centralized validation for decoded tarball size
            FileStreamValidator::validate_package_upload(&tarball_data, filename, "NPM")?;

            // Save tarball
            let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
            storage::save_file(tarball_path, &tarball_data).await?;

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

            // Save metadata
            let metadata_path = state
                .data_dir
                .join("npm/metadata")
                .join(format!("{}.json", package));
            let metadata_str = serde_json::to_string_pretty(&payload)?;
            storage::save_file(metadata_path, metadata_str.as_bytes()).await?;

            info!(package = %package, filename = %filename, size = tarball_data.len(), "npm package published successfully");
            return Ok(Json(SuccessResponse {
                message: "Package published successfully".to_string(),
            }));
        }
    }

    warn!(package = %package, "No valid tarball found in npm publish payload");
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
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use base64::engine::general_purpose;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use vm_package_server::{AppState, UpstreamClient, UpstreamConfig};

    fn create_npm_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("npm/tarballs")).unwrap();
        std::fs::create_dir_all(data_dir.join("npm/metadata")).unwrap();

        let upstream_config = UpstreamConfig::default();
        let config = Arc::new(vm_package_server::config::Config::default());
        let state = Arc::new(AppState {
            data_dir,
            server_addr: "http://localhost:8080".to_string(),
            upstream_client: Arc::new(UpstreamClient::new(upstream_config).unwrap()),
            config,
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

    #[tokio::test]
    async fn test_publish_package_with_tarball() {
        let (state, _temp_dir) = create_npm_test_state();
        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::put(publish_package))
            .with_state(state.clone());

        let server = TestServer::new(app).unwrap();

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
        let saved_content = std::fs::read(tarball_path).unwrap();
        assert_eq!(saved_content, tarball_content);

        // Verify metadata was saved
        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{}.json", package_name));
        assert!(metadata_path.exists());
        let metadata_content = std::fs::read_to_string(metadata_path).unwrap();
        let metadata: Value = serde_json::from_str(&metadata_content).unwrap();

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

        let server = TestServer::new(app).unwrap();

        let payload = json!({
            "name": "test-package",
            "version": "1.0.0"
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
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::get(package_metadata))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server.get(&format!("/npm/{}", package_name)).await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body: Value = response.json();
        assert_eq!(body["name"], package_name);
        assert_eq!(body["dist-tags"]["latest"], "1.0.0");
        assert!(body["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .unwrap()
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
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let app = axum::Router::new()
            .route("/npm/{package}", axum::routing::get(package_metadata))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server
            .get(&format!("/npm/{}", package_name))
            .add_header("host", "example.com:3000")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body: Value = response.json();

        // Verify the tarball URL uses static server_addr now
        let tarball_url = body["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert!(tarball_url.contains("localhost:8080"));
    }

    #[tokio::test]
    async fn test_download_tarball() {
        let (state, _temp_dir) = create_npm_test_state();

        // Create test tarball file
        let content = b"test tarball content";
        let filename = "test-package-1.0.0.tgz";
        let tarball_path = state.data_dir.join("npm/tarballs").join(filename);
        std::fs::write(&tarball_path, content).unwrap();

        let app = axum::Router::new()
            .route(
                "/npm/{package}/-/{filename}",
                axum::routing::get(download_tarball),
            )
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server
            .get("/npm/test-package/-/test-package-1.0.0.tgz")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.as_bytes().to_vec(), content.to_vec());
    }
}
