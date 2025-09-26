use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, info, warn};

use crate::deletion::{remove_version_from_index, update_index_yank_version};
use vm_package_server::validation_utils::FileStreamValidator;
use vm_package_server::{
    package_utils, sha256_hash, storage, validation, AppError, AppResult, AppState, SuccessResponse,
};

/// Metadata extracted from a crate upload
#[derive(Debug)]
struct CrateMetadata {
    name: String,
    version: String,
    deps: Value,
    features: Value,
}

/// Calculate Cargo index path for a crate name according to Cargo's index structure
/// Names are organized in directories: 1/a, 2/ab, 3/a/abc, ab/cd/abcd...
///
/// # Security
/// This function validates the crate name and performs bounds checking to prevent
/// directory traversal attacks and panics from malformed input.
fn index_path(name: &str) -> AppResult<String> {
    // Validate the crate name first
    let validated_name = validation::validate_package_name(name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{}': {}", name, e)))?;

    let name = validated_name.to_lowercase();

    // Perform bounds checking for string slicing operations
    let path = match name.len() {
        0 => {
            return Err(AppError::BadRequest(
                "Crate name cannot be empty".to_string(),
            ))
        }
        1 => format!("1/{}", name),
        2 => format!("2/{}", name),
        3 => {
            // Safe: we know name.len() == 3, so [..1] is valid
            let first_char = &name[..1];
            format!("3/{}/{}", first_char, name)
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
            format!("{}/{}/{}", first_two, next_two, name)
        }
    };

    // Validate the constructed path for safety
    validation::validate_safe_path(&path).map_err(|e| {
        AppError::BadRequest(format!("Generated unsafe index path '{}': {}", path, e))
    })?;

    Ok(path)
}

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
                    let filename = format!("{}-{}.crate", crate_name, version);

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
///
/// This endpoint provides the configuration information that Cargo clients need
/// to interact with this registry. It returns the download URL template and API
/// base URL according to the Cargo registry specification.
///
/// # Route
/// `GET /cargo/config.json`
///
/// # Returns
/// JSON configuration object with registry endpoints
///
/// # Example Response
/// ```json
/// {
///   "dl": "http://localhost:8080/cargo/api/v1/crates/{crate}/{version}/download",
///   "api": "http://localhost:8080/cargo"
/// }
/// ```
///
/// # Usage
/// Cargo clients use this configuration to:
/// - Download crate files via the `dl` URL template
/// - Make API calls to the `api` base URL
/// - Configure the registry in `.cargo/config.toml`
///
/// # URL Templates
/// - `{crate}` is replaced with the crate name
/// - `{version}` is replaced with the version string
pub async fn config(State(state): State<Arc<AppState>>) -> AppResult<Json<Value>> {
    debug!("Incoming Cargo config request");
    let host = &state.server_addr;

    Ok(Json(json!({
        "dl": format!("{}/cargo/api/v1/crates/{{crate}}/{{version}}/download", host),
        "api": format!("{}/cargo", host)
    })))
}

/// Serves Cargo index files containing crate version metadata.
///
/// This endpoint serves index files according to Cargo's index structure, returning
/// newline-delimited JSON containing metadata for all versions of a crate. Falls back
/// to upstream crates.io if the crate is not found locally.
///
/// # Route
/// `GET /cargo/{path}`
///
/// # Parameters
/// * `path` - Index path following Cargo's directory structure:
///   - `1/a` for single-letter crates
///   - `2/ab` for two-letter crates
///   - `3/a/abc` for three-letter crates
///   - `ab/cd/abcd` for longer crates
///
/// # Returns
/// Newline-delimited JSON string with crate version entries
///
/// # Example Response
/// ```
/// {"name":"serde","vers":"1.0.0","deps":[],"cksum":"abc123","features":{},"yanked":false}
/// {"name":"serde","vers":"1.0.1","deps":[],"cksum":"def456","features":{},"yanked":false}
/// ```
///
/// # Index Structure
/// - Each line represents one version of the crate
/// - JSON objects contain version metadata including dependencies
/// - `cksum` field contains SHA256 hash of the crate file
/// - `yanked` field indicates if version is yanked (deprecated)
///
/// # Fallback Behavior
/// 1. First attempts to serve from local index (`cargo/index/`)
/// 2. If not found locally, fetches from upstream crates.io
/// 3. Returns appropriate error if crate not found anywhere
pub async fn index_file(
    axum::extract::Path(path): axum::extract::Path<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<String> {
    // Extract crate name from the path (e.g., "2/ab" or "3/a/bc" or "ab/cd/abcd")
    let crate_name = path
        .split('/')
        .next_back()
        .ok_or_else(|| AppError::BadRequest(format!("Cargo index path format is invalid: '{}' - expected format: 1/a, 2/ab, 3/a/abc, or ab/cd/abcd...", path)))?;

    // Validate the extracted crate name for security
    validation::validate_package_name(crate_name, "cargo").map_err(|e| {
        AppError::BadRequest(format!(
            "Invalid crate name '{}' extracted from path '{}': {}",
            crate_name, path, e
        ))
    })?;

    debug!(crate_name = %crate_name, path = %path, "Incoming Cargo index request");
    info!(crate_name = %crate_name, "Fetching Cargo index file");
    let index_path_str = index_path(crate_name)?;
    let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);

    // Try local index first
    match storage::read_file_string(&index_file_path).await {
        Ok(content) => {
            debug!(crate_name = %crate_name, "Serving index from local storage");
            Ok(content)
        }
        Err(_) => {
            // Index not found locally, try upstream crates.io
            debug!(crate_name = %crate_name, "Index not found locally, checking upstream crates.io");
            match state
                .upstream_client
                .fetch_cargo_index(crate_name, &path)
                .await
            {
                Ok(upstream_content) => {
                    info!(crate_name = %crate_name, "Found crate on upstream crates.io, returning index");
                    Ok(upstream_content)
                }
                Err(e) => {
                    debug!(crate_name = %crate_name, error = %e, "Crate not found on upstream crates.io either");
                    Err(e)
                }
            }
        }
    }
}

/// Downloads Cargo crate files from local storage or upstream registry.
///
/// This endpoint serves Cargo crate files (.crate format) with fallback to the
/// upstream crates.io registry if the file is not found locally. It supports
/// transparent proxying of crates from the official Cargo registry.
///
/// # Route
/// `GET /cargo/api/v1/crates/{crate}/{version}/download`
///
/// # Parameters
/// * `crate_name` - The Cargo crate name
/// * `version` - The specific version to download (e.g., "1.0.0")
///
/// # Returns
/// Binary crate data as `Vec<u8>` (.crate file format)
///
/// # Example Request
/// ```
/// GET /cargo/api/v1/crates/serde/1.0.0/download
/// ```
///
/// # File Format
/// - Crate files use the `.crate` extension
/// - Files are stored as `{crate_name}-{version}.crate`
/// - Contains compressed source code and metadata
///
/// # Behavior
/// 1. First attempts to serve from local storage (`cargo/crates/`)
/// 2. If not found locally, streams from upstream crates.io
/// 3. Returns appropriate error if crate not found anywhere
///
/// # Error Conditions
/// - Crate or version not found locally or upstream
/// - Network errors when contacting upstream
/// - File system access errors
pub async fn download_crate(
    AxumPath((crate_name, version)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    debug!(crate_name = %crate_name, version = %version, "Incoming Cargo crate download request");

    // Validate crate name and version for security
    validation::validate_package_name(&crate_name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{}': {}", crate_name, e)))?;
    validation::validate_version(&version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version '{}': {}", version, e)))?;

    info!(crate_name = %crate_name, version = %version, "Downloading Cargo crate");
    let filename = format!("{}-{}.crate", crate_name, version);

    // Validate the constructed filename for security
    validation::validate_safe_path(&filename).map_err(|e| {
        AppError::BadRequest(format!("Generated unsafe filename '{}': {}", filename, e))
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

/// Parse and validate crate upload payload with comprehensive size and structure validation
/// Returns (CrateMetadata, crate_data)
fn parse_crate_upload(body: axum::body::Bytes) -> AppResult<(CrateMetadata, Vec<u8>)> {
    let data = &body[..];
    let payload_size = data.len();

    // Validate minimum payload size for headers
    if payload_size < 8 {
        warn!(
            payload_size = payload_size,
            "Cargo publish payload too small"
        );
        return Err(AppError::UploadError(
            "Payload too small - missing required headers".to_string(),
        ));
    }

    // Validate total payload size using our centralized validation
    validation::validate_file_size(
        payload_size as u64,
        Some(validation::MAX_REQUEST_BODY_SIZE as u64),
    )
    .map_err(|e| {
        warn!(
            payload_size = payload_size,
            "Cargo payload size validation failed"
        );
        AppError::UploadError(format!("Payload too large: {}", e))
    })?;

    // Parse: 4-byte metadata length + JSON metadata + 4-byte crate length + .crate file
    let metadata_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

    // Validate metadata size
    if metadata_len > validation::MAX_METADATA_SIZE {
        warn!(
            metadata_len = metadata_len,
            "Cargo metadata section too large"
        );
        return Err(AppError::UploadError(format!(
            "Metadata section too large: {} bytes (max: {} bytes)",
            metadata_len,
            validation::MAX_METADATA_SIZE
        )));
    }

    if payload_size < 4 + metadata_len + 4 {
        warn!(
            payload_size = payload_size,
            metadata_len = metadata_len,
            "Cargo publish payload insufficient for metadata"
        );
        return Err(AppError::UploadError(
            "Insufficient data for metadata section".to_string(),
        ));
    }

    // Extract and validate JSON metadata
    let metadata_bytes = &data[4..4 + metadata_len];
    let metadata: Value = serde_json::from_slice(metadata_bytes).map_err(|e| {
        warn!(error = %e, "Failed to parse Cargo metadata JSON");
        AppError::UploadError(format!("Invalid metadata JSON: {}", e))
    })?;

    let crate_name = metadata["name"].as_str().ok_or_else(|| {
        warn!("Cargo publish metadata name field missing or not a string");
        AppError::UploadError("'name' field missing or not a string in metadata".to_string())
    })?;
    let version = metadata["vers"].as_str().ok_or_else(|| {
        warn!("Cargo publish metadata vers field missing or not a string");
        AppError::UploadError("'vers' field missing or not a string in metadata".to_string())
    })?;

    // Validate crate name and version early
    validation::validate_package_name(crate_name, "cargo").map_err(|e| {
        warn!(crate_name = %crate_name, error = %e, "Invalid crate name in upload");
        AppError::BadRequest(format!("Invalid crate name: {}", e))
    })?;
    validation::validate_version(version).map_err(|e| {
        warn!(version = %version, error = %e, "Invalid version in upload");
        AppError::BadRequest(format!("Invalid version: {}", e))
    })?;

    // Extract .crate file length
    let crate_len_offset = 4 + metadata_len;
    if payload_size < crate_len_offset + 4 {
        warn!(
            payload_size = payload_size,
            offset = crate_len_offset,
            "Payload too small for crate length header"
        );
        return Err(AppError::UploadError(
            "Payload missing crate length header".to_string(),
        ));
    }

    let crate_len = u32::from_le_bytes([
        data[crate_len_offset],
        data[crate_len_offset + 1],
        data[crate_len_offset + 2],
        data[crate_len_offset + 3],
    ]) as usize;

    // Use centralized validation for crate file size
    FileStreamValidator::validate_total_upload_size(crate_len as u64, "Cargo")?;

    let crate_data_offset = crate_len_offset + 4;
    if payload_size < crate_data_offset + crate_len {
        warn!(
            payload_size = payload_size,
            expected_size = crate_data_offset + crate_len,
            "Cargo publish payload insufficient for crate data"
        );
        return Err(AppError::UploadError(
            "Insufficient data for crate file".to_string(),
        ));
    }

    // Validate the overall structure using our centralized validation
    validation::validate_cargo_upload_structure(payload_size, metadata_len, crate_len).map_err(
        |e| {
            warn!(error = %e, "Cargo upload structure validation failed");
            AppError::UploadError(format!("Invalid upload structure: {}", e))
        },
    )?;

    // Extract .crate file
    let crate_data = &data[crate_data_offset..crate_data_offset + crate_len];
    debug!(crate_size = crate_data.len(), "Extracted crate file data");

    // Use centralized validation for extracted crate package
    FileStreamValidator::validate_package_upload(crate_data, "crate", "Cargo")?;

    let crate_metadata = CrateMetadata {
        name: crate_name.to_string(),
        version: version.to_string(),
        // Safe: unwrap_or provides sensible defaults for optional metadata fields
        // deps defaults to empty array, features defaults to empty object
        deps: metadata.get("deps").unwrap_or(&json!([])).clone(),
        features: metadata.get("features").unwrap_or(&json!({})).clone(),
    };

    Ok((crate_metadata, crate_data.to_vec()))
}

/// Save crate file to the appropriate directory
async fn save_crate_file(
    data: &[u8],
    crate_name: &str,
    version: &str,
    data_dir: &std::path::Path,
) -> AppResult<PathBuf> {
    // Validate inputs for security
    validation::validate_package_name(crate_name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{}': {}", crate_name, e)))?;
    validation::validate_version(version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version '{}': {}", version, e)))?;

    let filename = format!("{}-{}.crate", crate_name, version);

    // Validate the constructed filename path
    validation::validate_safe_path(&filename).map_err(|e| {
        AppError::BadRequest(format!("Generated unsafe filename '{}': {}", filename, e))
    })?;

    let crate_path = data_dir.join("cargo/crates").join(&filename);

    storage::save_file(&crate_path, data).await?;

    Ok(crate_path)
}

/// Update the crate index with new crate information
async fn update_crate_index(
    metadata: &CrateMetadata,
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

    // Append to index file using storage utility
    let index_line = serde_json::to_string(&index_entry)?;
    storage::append_to_file(index_file_path, &index_line).await?;

    Ok(())
}

/// Publishes a new Cargo crate version to the local registry.
///
/// This endpoint handles Cargo crate publishing according to the Cargo registry API.
/// It processes binary uploads containing crate metadata and .crate file data,
/// validates the content, and updates both the index and crate storage.
///
/// # Route
/// `PUT /cargo/api/v1/crates/new`
///
/// # Request Body
/// Binary payload with the following structure:
/// 1. 4-byte little-endian metadata length
/// 2. JSON metadata
/// 3. 4-byte little-endian crate file length
/// 4. .crate file data
///
/// # Metadata Structure
/// ```json
/// {
///   "name": "crate-name",
///   "vers": "1.0.0",
///   "deps": [],
///   "features": {},
///   "authors": ["author@example.com"],
///   "description": "Crate description",
///   "license": "MIT"
/// }
/// ```
///
/// # Returns
/// JSON success response confirming crate publication
///
/// # Example Response
/// ```json
/// {
///   "message": "Crate published successfully"
/// }
/// ```
///
/// # Processing Steps
/// 1. Parses binary payload to extract metadata and .crate file
/// 2. Validates crate name and version from metadata
/// 3. Calculates SHA256 checksum of crate file
/// 4. Saves .crate file to `cargo/crates/` directory
/// 5. Updates index file with new version entry
/// 6. Appends newline-delimited JSON entry to index
///
/// # Error Conditions
/// - Malformed binary payload
/// - Invalid or missing metadata fields
/// - File system write errors
/// - Index update failures
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
///
/// This endpoint supports both yanking (marking as deprecated) and force deletion
/// of specific crate versions. Yanking is the standard Cargo operation that preserves
/// the crate file but marks it as unavailable for new downloads.
///
/// # Route
/// `DELETE /cargo/{crate_name}/{version}[?force=true]`
///
/// # Parameters
/// * `crate_name` - The Cargo crate name
/// * `version` - The specific version to yank or delete
/// * `force` - Query parameter: if true, force delete; if false/missing, yank
///
/// # Query Parameters
/// * `force=true` - Permanently delete the version and .crate file
/// * `force=false` or omitted - Yank the version (mark as unavailable)
///
/// # Returns
/// JSON response confirming the operation
///
/// # Example Responses
/// Yank operation:
/// ```json
/// {
///   "message": "Yanked version 1.0.0 of crate 'my-crate'"
/// }
/// ```
///
/// Force delete operation:
/// ```json
/// {
///   "message": "Force deleted version 1.0.0 of crate 'my-crate'"
/// }
/// ```
///
/// # Operations
///
/// ## Yank (default behavior)
/// - Updates index entry to set `"yanked": true`
/// - Preserves the .crate file
/// - Version becomes unavailable for new downloads
/// - Existing dependencies continue to work
///
/// ## Force Delete (`force=true`)
/// - Removes the version entry from the index completely
/// - Deletes the .crate file from storage
/// - Version becomes completely unavailable
/// - May break existing dependencies
///
/// # Error Conditions
/// - Crate or version not found
/// - Index file access errors
/// - File system errors during deletion
pub async fn delete_crate_version(
    AxumPath((crate_name, version)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
    Query(params): Query<DeleteParams>,
) -> AppResult<Json<SuccessResponse>> {
    // Validate crate name and version for security
    validation::validate_package_name(&crate_name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{}': {}", crate_name, e)))?;
    validation::validate_version(&version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version '{}': {}", version, e)))?;

    let action = if params.force { "Deleting" } else { "Yanking" };
    info!(crate_name = %crate_name, version = %version, action = action, "Processing Cargo crate");

    let index_path_str = index_path(&crate_name)?;
    let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);

    if params.force {
        // Force delete: remove from index and delete .crate file
        let crate_file = state
            .data_dir
            .join("cargo/crates")
            .join(format!("{}-{}.crate", crate_name, version));

        // Remove the specific version from index
        remove_version_from_index(&index_file_path, &version).await?;

        // Delete the .crate file
        if crate_file.exists() {
            tokio::fs::remove_file(&crate_file).await?;
            info!(crate_file = %crate_file.display(), "Deleted crate file");
        }

        Ok(Json(SuccessResponse {
            message: format!(
                "Force deleted version {} of crate '{}'",
                version, crate_name
            ),
        }))
    } else {
        // Yank: just mark as yanked in index
        update_index_yank_version(&index_file_path, &version).await?;

        Ok(Json(SuccessResponse {
            message: format!("Yanked version {} of crate '{}'", version, crate_name),
        }))
    }
}

/// Placeholder API endpoint for crates.io compatibility
pub async fn crates_io_api_placeholder() -> AppResult<Response> {
    // Return minimal valid response for Cargo's API version check
    Ok((StatusCode::OK, Json(json!({"api_version": "v1"}))).into_response())
}

/// Sparse index handler for Cargo registries
///
/// This serves the sparse index format which is HTTP-based instead of Git-based.
/// For sparse indexes, Cargo requests individual crate metadata files directly
/// via HTTP rather than cloning a Git repository.
pub async fn sparse_index(
    AxumPath(path): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Response> {
    // Handle config.json separately
    if path == "config.json" {
        return config(State(state)).await.map(|json| json.into_response());
    }

    // For sparse index, the path format is different:
    // - 2-char prefix: /ca/rg/cargo
    // - 3-char: /3/c/cargo
    // - 4+ char: first 2 chars / next 2 chars / crate_name

    // Extract the crate name from the path
    let parts: Vec<&str> = path.split('/').collect();
    let crate_name = parts
        .last()
        .ok_or_else(|| AppError::BadRequest(format!("Invalid sparse index path: {}", path)))?;

    debug!(crate_name = %crate_name, path = %path, "Sparse index request");

    // Use the existing index_file logic
    index_file(AxumPath(path), State(state))
        .await
        .map(|content| content.into_response())
}

/// Deletes all versions of a Cargo crate from the registry.
///
/// This endpoint completely removes a Cargo crate and all its versions from the
/// local registry. It deletes both the index file and all associated .crate files.
/// This is a destructive operation that completely removes the crate.
///
/// # Route
/// `DELETE /cargo/crates/{crate_name}`
///
/// # Parameters
/// * `crate_name` - The Cargo crate name to completely remove
///
/// # Returns
/// JSON response with details of deleted files
///
/// # Example Response
/// ```json
/// {
///   "message": "Deleted 4 files for Cargo crate 'my-crate'"
/// }
/// ```
///
/// # Processing Steps
/// 1. Deletes all .crate files matching the crate name pattern
/// 2. Deletes the index file for the crate
/// 3. Returns count of deleted files
///
/// # File Pattern Matching
/// - Crate files: `{crate_name}-*.crate` in `cargo/crates/`
/// - Index file: Calculated path in `cargo/index/` based on crate name
///
/// # Index Path Calculation
/// Index paths follow Cargo's structure:
/// - 1-letter: `1/{name}`
/// - 2-letter: `2/{name}`
/// - 3-letter: `3/{first_char}/{name}`
/// - 4+ letter: `{first_2_chars}/{next_2_chars}/{name}`
///
/// # Error Conditions
/// - Crate not found (no files to delete)
/// - File system access errors
/// - Partial deletion failures (logged as warnings)
///
/// # Backward Compatibility
/// This endpoint maintains compatibility with older deletion APIs and provides
/// a way to completely clean up a crate from the registry.
pub async fn delete_all_versions(
    AxumPath(crate_name): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<SuccessResponse>> {
    // Validate crate name for security
    validation::validate_package_name(&crate_name, "cargo")
        .map_err(|e| AppError::BadRequest(format!("Invalid crate name '{}': {}", crate_name, e)))?;

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
                if filename.starts_with(&format!("{}-", crate_name)) && filename.ends_with(".crate")
                {
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
            deleted_files.push(format!("index/{}", crate_name));
        }
    }

    if deleted_files.is_empty() {
        warn!(crate_name = %crate_name, "No files found for crate");
        return Err(AppError::NotFound(format!(
            "Cargo crate '{}' not found",
            crate_name
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use vm_package_server::{AppState, UpstreamClient, UpstreamConfig};

    fn create_cargo_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("cargo/crates")).unwrap();
        std::fs::create_dir_all(data_dir.join("cargo/api/v1/crates")).unwrap();
        std::fs::create_dir_all(data_dir.join("cargo/index")).unwrap();

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

    fn create_cargo_publish_payload(
        crate_name: &str,
        version: &str,
        crate_content: &[u8],
    ) -> Vec<u8> {
        let metadata = json!({
            "name": crate_name,
            "vers": version,
            "deps": [],
            "features": {},
            "authors": ["test@example.com"],
            "description": "Test crate",
            "license": "MIT"
        });

        let metadata_bytes = serde_json::to_vec(&metadata).unwrap();
        let metadata_len = metadata_bytes.len() as u32;
        let crate_len = crate_content.len() as u32;

        let mut payload = Vec::new();

        // Add metadata length (little-endian)
        payload.extend_from_slice(&metadata_len.to_le_bytes());

        // Add metadata
        payload.extend_from_slice(&metadata_bytes);

        // Add crate length (little-endian)
        payload.extend_from_slice(&crate_len.to_le_bytes());

        // Add crate content
        payload.extend_from_slice(crate_content);

        payload
    }

    #[tokio::test]
    async fn test_publish_crate_with_binary_payload() {
        let (state, _temp_dir) = create_cargo_test_state();
        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state.clone());

        let server = TestServer::new(app).unwrap();

        let crate_name = "test-crate";
        let version = "1.0.0";
        let crate_content = b"fake crate content";
        let payload = create_cargo_publish_payload(crate_name, version, crate_content);

        let response = server
            .put("/cargo/api/v1/crates/new")
            .bytes(payload.into())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify .crate file was saved
        let crate_path = state
            .data_dir
            .join("cargo/crates")
            .join(format!("{}-{}.crate", crate_name, version));
        assert!(crate_path.exists());
        let saved_content = std::fs::read(crate_path).unwrap();
        assert_eq!(saved_content, crate_content);

        // Verify index file was created
        let index_path_str = index_path(crate_name).unwrap();
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);
        assert!(index_file_path.exists());

        let index_content = std::fs::read_to_string(index_file_path).unwrap();
        assert!(index_content.contains(crate_name));
        assert!(index_content.contains(version));
        assert!(index_content.contains("\"cksum\":"));
    }

    #[tokio::test]
    async fn test_publish_crate_appends_to_existing_index() {
        let (state, _temp_dir) = create_cargo_test_state();

        let crate_name = "test-crate";
        let index_path_str = index_path(crate_name).unwrap();
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);

        // Create directory and initial index entry
        std::fs::create_dir_all(index_file_path.parent().unwrap()).unwrap();
        let existing_entry = json!({
            "name": crate_name,
            "vers": "0.9.0",
            "deps": [],
            "cksum": "abc123",
            "features": {},
            "yanked": false
        });
        std::fs::write(
            &index_file_path,
            format!("{}\n", serde_json::to_string(&existing_entry).unwrap()),
        )
        .unwrap();

        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state.clone());

        let server = TestServer::new(app).unwrap();

        let version = "1.0.0";
        let crate_content = b"fake crate content";
        let payload = create_cargo_publish_payload(crate_name, version, crate_content);

        let response = server
            .put("/cargo/api/v1/crates/new")
            .bytes(payload.into())
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify index file contains both versions
        let index_content = std::fs::read_to_string(index_file_path).unwrap();
        let lines: Vec<&str> = index_content.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("0.9.0"));
        assert!(lines[1].contains("1.0.0"));
    }

    #[tokio::test]
    async fn test_publish_crate_rejects_malformed_payload() {
        let (state, _temp_dir) = create_cargo_test_state();
        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/new",
                axum::routing::put(publish_crate),
            )
            .with_state(state);

        let server = TestServer::new(app).unwrap();

        // Send malformed payload (too short)
        let payload = vec![1, 2, 3];

        let response = server
            .put("/cargo/api/v1/crates/new")
            .bytes(payload.into())
            .await;

        assert_eq!(response.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_cargo_config() {
        let (state, _temp_dir) = create_cargo_test_state();
        let app = axum::Router::new()
            .route("/cargo/config.json", axum::routing::get(config))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server
            .get("/cargo/config.json")
            .add_header("host", "example.com:3000")
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body: Value = response.json();

        let dl_url = body["dl"].as_str().unwrap();
        assert!(dl_url.contains("localhost:8080")); // Uses static server_addr now
        assert!(dl_url.contains("{crate}"));
        assert!(dl_url.contains("{version}"));
    }

    #[tokio::test]
    async fn test_index_file_after_publish() {
        let (state, _temp_dir) = create_cargo_test_state();

        let crate_name = "test-crate";
        let index_entry = json!({
            "name": crate_name,
            "vers": "1.0.0",
            "deps": [],
            "cksum": "abc123def456",
            "features": {},
            "yanked": false
        });

        let index_path_str = index_path(crate_name).unwrap();
        let index_file_path = state.data_dir.join("cargo/index").join(&index_path_str);
        std::fs::create_dir_all(index_file_path.parent().unwrap()).unwrap();
        std::fs::write(
            &index_file_path,
            format!("{}\n", serde_json::to_string(&index_entry).unwrap()),
        )
        .unwrap();

        let app = axum::Router::new()
            .route("/cargo/index/{crate}", axum::routing::get(index_file))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server.get(&format!("/cargo/index/{}", crate_name)).await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body = response.text();
        assert!(body.contains(crate_name));
        assert!(body.contains("1.0.0"));
        assert!(body.contains("abc123def456"));
    }

    #[tokio::test]
    async fn test_download_crate() {
        let (state, _temp_dir) = create_cargo_test_state();

        // Create test crate file
        let content = b"test crate content";
        let crate_name = "test-crate";
        let version = "1.0.0";
        let filename = format!("{}-{}.crate", crate_name, version);
        let crate_path = state.data_dir.join("cargo/crates").join(&filename);
        std::fs::write(&crate_path, content).unwrap();

        let app = axum::Router::new()
            .route(
                "/cargo/api/v1/crates/{crate}/{version}/download",
                axum::routing::get(download_crate),
            )
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server
            .get(&format!(
                "/cargo/api/v1/crates/{}/{}/download",
                crate_name, version
            ))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.as_bytes().to_vec(), content.to_vec());
    }

    #[test]
    fn test_index_path_algorithm() {
        assert_eq!(index_path("a").unwrap(), "1/a");
        assert_eq!(index_path("ab").unwrap(), "2/ab");
        assert_eq!(index_path("abc").unwrap(), "3/a/abc");
        assert_eq!(index_path("abcd").unwrap(), "ab/cd/abcd");
        assert_eq!(index_path("serde").unwrap(), "se/rd/serde");
        assert_eq!(index_path("SERDE").unwrap(), "se/rd/serde"); // test lowercase conversion

        // Test security: invalid characters should be rejected
        assert!(index_path("../malicious").is_err());
        assert!(index_path("crate/../etc/passwd").is_err());
        assert!(index_path("/absolute/path").is_err());
        assert!(index_path("crate with spaces").is_err());
        assert!(index_path("").is_err());
    }
}
