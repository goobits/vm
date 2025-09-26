use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    extract::{Multipart, Path as AxumPath, State},
    response::Html,
};
use tracing::{debug, info, warn};

use crate::validation;
use vm_package_server::validation_utils::FileStreamValidator;
use vm_package_server::{
    normalize_pypi_name, package_utils, sha256_hash, storage, validate_filename, AppError,
    AppResult, AppState, SuccessResponse,
};

/// Helper to list valid package files (.whl, .tar.gz) in the PyPI packages directory
async fn list_package_files(pypi_dir: &Path) -> AppResult<Vec<PathBuf>> {
    package_utils::list_files_with_extensions(pypi_dir, &[".whl", ".tar.gz"]).await
}

/// Returns the PyPI simple package index as HTML.
///
/// This endpoint implements the PEP 503 simple repository API, providing a list of all
/// available packages in HTML format. Each package name is normalized according to PEP 503
/// rules and presented as a clickable link.
///
/// # Route
/// `GET /pypi/simple/`
///
/// # Returns
/// HTML page containing links to all available packages
///
/// # Example Response
/// ```html
/// <!DOCTYPE html>
/// <html>
///   <head><title>Simple index</title></head>
///   <body>
///     <h1>Simple index</h1>
///     <a href="package-name/">package-name</a><br/>
///   </body>
/// </html>
/// ```
pub async fn simple_index(State(state): State<Arc<AppState>>) -> AppResult<Html<String>> {
    info!("Generating PyPI simple index");
    let pypi_dir = state.data_dir.join("pypi/packages");

    let mut packages = std::collections::HashSet::new();

    for path in list_package_files(&pypi_dir).await? {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(pkg_name) = name.split('-').next() {
                packages.insert(normalize_pypi_name(pkg_name));
            }
        }
    }

    let mut html = String::from(
        r#"<!DOCTYPE html>
<html>
  <head><title>Simple index</title></head>
  <body>
    <h1>Simple index</h1>
"#,
    );

    for package in packages {
        html.push_str(&format!(r#"    <a href="{package}/">{package}</a><br/>"#));
    }

    html.push_str("  </body>\n</html>");
    Ok(Html(html))
}

/// Returns download links for all versions of a specific PyPI package.
///
/// This endpoint implements the PEP 503 simple repository API for individual packages,
/// providing download links for all available versions with SHA256 hashes for integrity
/// verification.
///
/// # Route
/// `GET /pypi/simple/{package}/`
///
/// # Parameters
/// - `package`: The package name (will be normalized according to PEP 503)
///
/// # Returns
/// HTML page containing download links for all package versions with SHA256 hashes
///
/// # Example Response
/// ```html
/// <!DOCTYPE html>
/// <html>
///   <head><title>Links for package-name</title></head>
///   <body>
///     <h1>Links for package-name</h1>
///     <a href="../../packages/package_name-1.0.0-py3-none-any.whl#sha256=abcd...">package_name-1.0.0-py3-none-any.whl</a><br/>
///   </body>
/// </html>
/// ```
pub async fn package_index(
    AxumPath(package): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Html<String>> {
    info!(package = %package, "Generating PyPI package index");
    let normalized_package = normalize_pypi_name(&package);
    let pypi_dir = state.data_dir.join("pypi/packages");
    let host = &state.server_addr;

    let mut files = Vec::new();

    for path in list_package_files(&pypi_dir).await? {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(pkg_name) = name.split('-').next() {
                if normalize_pypi_name(pkg_name) == normalized_package {
                    let meta_path = path.with_extension(format!(
                        "{}.meta",
                        path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
                    ));
                    match storage::read_file_string(&meta_path).await {
                        Ok(hash) => {
                            files.push((name.to_string(), hash.trim().to_string()));
                            debug!(filename = %name, hash = %hash.trim(), "Added file to package index from meta");
                        }
                        Err(_) => {
                            warn!(package = %package, path = %path.display(), meta_path = %meta_path.display(), "Meta file not found for package, skipping hash");
                        }
                    }
                }
            }
        }
    }

    // If no local files found, try upstream PyPI
    if files.is_empty() {
        debug!(package = %package, "No local files found, checking upstream PyPI");
        match state
            .upstream_client
            .fetch_pypi_simple(&normalized_package)
            .await
        {
            Ok(upstream_html) => {
                info!(package = %package, "Found package on upstream PyPI, proxying response");
                return Ok(Html(upstream_html));
            }
            Err(_) => {
                debug!(package = %package, "Package not found on upstream PyPI either");
            }
        }
    }

    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
  <head><title>Links for {package}</title></head>
  <body>
    <h1>Links for {package}</h1>
"#
    );

    for (filename, hash) in files {
        html.push_str(&format!(
            r#"    <a href="{host}/pypi/packages/{filename}#sha256={hash}">{filename}</a><br/>"#
        ));
    }

    html.push_str("  </body>\n</html>");
    Ok(Html(html))
}

/// Count total number of PyPI packages
/// Counts the total number of unique PyPI packages in the repository.
///
/// Scans the PyPI packages directory for .whl and .tar.gz files, extracts package names,
/// normalizes them according to PEP 503, and returns the count of unique packages.
///
/// # Arguments
/// * `state` - Application state containing the data directory path
///
/// # Returns
/// The number of unique packages stored in the repository
///
/// # Example
/// ```rust
/// let count = count_packages(&app_state).await?;
/// println!("Repository contains {} packages", count);
/// ```
///
/// **Deprecated**: Use `get_registry("pypi")?.count_packages(state)` via trait system instead.
#[deprecated(
    since = "0.1.0",
    note = "Use Registry trait method via get_registry() instead"
)]
#[allow(dead_code)]
pub async fn count_packages(state: &AppState) -> AppResult<usize> {
    package_utils::count_packages_by_pattern(
        &state.data_dir,
        &package_utils::RegistryPattern::PYPI,
        |filename| {
            // Extract package name (first part before hyphen) and normalize it
            filename.split('-').next().map(normalize_pypi_name)
        },
    )
    .await
}

/// Returns a sorted list of all unique PyPI package names in the repository.
///
/// Scans the PyPI packages directory for .whl and .tar.gz files, extracts and normalizes
/// package names according to PEP 503, and returns them in alphabetical order.
///
/// # Arguments
/// * `state` - Application state containing the data directory path
///
/// # Returns
/// A vector of normalized package names sorted alphabetically
///
/// # Example
/// ```rust
/// let packages = list_all_packages(&app_state).await?;
/// for package in packages {
///     println!("Package: {}", package);
/// }
/// ```
///
/// **Deprecated**: Use `get_registry("pypi")?.list_all_packages(state)` via trait system instead.
#[deprecated(
    since = "0.1.0",
    note = "Use Registry trait method via get_registry() instead"
)]
#[allow(dead_code)]
pub async fn list_all_packages(state: &AppState) -> AppResult<Vec<String>> {
    package_utils::list_packages_by_pattern(
        &state.data_dir,
        &package_utils::RegistryPattern::PYPI,
        |filename| {
            // Extract package name (first part before hyphen) and normalize it
            filename.split('-').next().map(normalize_pypi_name)
        },
    )
    .await
}

/// Get package versions with their files, hashes, and sizes
pub async fn get_package_versions(
    state: &AppState,
    package_name: &str,
) -> AppResult<Vec<(String, Vec<(String, String, u64)>)>> {
    let normalized_package = normalize_pypi_name(package_name);
    let pypi_dir = state.data_dir.join("pypi/packages");
    let mut versions_map: HashMap<String, Vec<(String, String, u64)>> = HashMap::new();

    for path in list_package_files(&pypi_dir).await? {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(pkg_name) = name.split('-').next() {
                if normalize_pypi_name(pkg_name) == normalized_package {
                    // Extract version from filename (package-version-...)
                    let version =
                        if let Some(version_start) = name.strip_prefix(&format!("{}-", pkg_name)) {
                            version_start
                                .split('-')
                                .next()
                                .unwrap_or("unknown")
                                .to_string()
                        } else {
                            "unknown".to_string()
                        };

                    let meta_path = path.with_extension(format!(
                        "{}.meta",
                        path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
                    ));

                    let hash = storage::read_file_string(&meta_path)
                        .await
                        .unwrap_or_else(|_| "unknown".to_string());

                    // Get file size
                    let size = package_utils::get_file_size(&path).await;

                    versions_map.entry(version).or_default().push((
                        name.to_string(),
                        hash.trim().to_string(),
                        size,
                    ));
                }
            }
        }
    }

    // Type alias for complex return type
    type VersionData = Vec<(String, String, u64)>;
    let mut versions: Vec<(String, VersionData)> = versions_map.into_iter().collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0)); // Sort versions in reverse order
    Ok(versions)
}

/// Get recent packages (returns up to `limit` most recent packages)
pub async fn get_recent_packages(
    state: &AppState,
    limit: usize,
) -> AppResult<Vec<(String, String)>> {
    // Get raw recent files using pattern helper, then apply PyPI deduplication
    let recent_files = package_utils::get_recent_packages_by_pattern(
        &state.data_dir,
        &package_utils::RegistryPattern::PYPI,
        limit, // Helper already handles limit * 2 internally for deduplication
        |filename| {
            if let Some(pkg_name) = filename.split('-').next() {
                let normalized = normalize_pypi_name(pkg_name);
                let version =
                    if let Some(version_start) = filename.strip_prefix(&format!("{}-", pkg_name)) {
                        version_start
                            .split('-')
                            .next()
                            .unwrap_or("unknown")
                            .to_string()
                    } else {
                        "unknown".to_string()
                    };
                Some((normalized, version))
            } else {
                None
            }
        },
    )
    .await?;

    // Deduplicate by package name (keep first occurrence which is most recent)
    let mut seen_packages = HashSet::new();
    let mut recent = Vec::new();

    for (package_name, version) in recent_files {
        if !seen_packages.contains(&package_name) {
            seen_packages.insert(package_name.clone());
            recent.push((package_name, version));
            if recent.len() >= limit {
                break;
            }
        }
    }

    Ok(recent)
}

/// Downloads a specific PyPI package file.
///
/// Serves package files (.whl, .tar.gz) with fallback to upstream PyPI if the file
/// is not found locally. Validates filename to prevent path traversal attacks.
///
/// # Route
/// `GET /pypi/packages/{filename}`
///
/// # Parameters
/// * `filename` - The package filename to download
///
/// # Returns
/// Binary content of the requested package file
///
/// # Security
/// - Validates filename to prevent directory traversal
/// - Only serves files from the designated packages directory
///
/// # Example
/// ```
/// GET /pypi/packages/package-name-1.0.0-py3-none-any.whl
/// ```
pub async fn download_file(
    AxumPath(filename): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    // Validate filename to prevent path traversal
    validate_filename(&filename)?;

    info!(filename = %filename, "Downloading PyPI package file");
    let file_path = state.data_dir.join("pypi/packages").join(&filename);

    // Try local file first
    match storage::read_file(&file_path).await {
        Ok(data) => {
            debug!(filename = %filename, size = data.len(), "Serving file from local storage");
            Ok(data)
        }
        Err(_) => {
            // File not found locally, try upstream PyPI
            debug!(filename = %filename, "File not found locally, checking upstream PyPI");
            match state.upstream_client.stream_pypi_file(&filename).await {
                Ok(bytes) => {
                    info!(filename = %filename, size = bytes.len(), "Streaming file from upstream PyPI");
                    Ok(bytes.to_vec())
                }
                Err(e) => {
                    debug!(filename = %filename, error = %e, "File not found on upstream PyPI either");
                    Err(e)
                }
            }
        }
    }
}

/// Uploads a new PyPI package to the repository.
///
/// Accepts multipart form data containing package files (.whl, .tar.gz) and stores them
/// in the PyPI packages directory. Generates SHA256 hashes for integrity verification.
///
/// # Route
/// `POST /pypi/`
///
/// # Request Body
/// Multipart form data with package files
///
/// # Returns
/// JSON success response with upload confirmation
///
/// # Supported Formats
/// - `.whl` files (Python wheels)
/// - `.tar.gz` files (source distributions)
///
/// # Example Response
/// ```json
/// {
///   "message": "Package uploaded successfully",
///   "status": "success"
/// }
/// ```
pub async fn upload_package(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> AppResult<axum::Json<SuccessResponse>> {
    info!("Processing PyPI package upload");
    let pypi_dir = state.data_dir.join("pypi/packages");

    let mut field_count = 0;
    let mut total_size = 0u64;

    while let Some(field) = multipart.next_field().await? {
        field_count += 1;
        let name = field.name().unwrap_or("").to_string();
        debug!(field_name = %name, "Processing multipart field");

        // Validate multipart limits early to prevent resource exhaustion
        if field_count > validation::MAX_MULTIPART_FIELDS {
            warn!(field_count = %field_count, "Too many multipart fields");
            return Err(AppError::UploadError(format!(
                "Too many multipart fields: {} (max: {})",
                field_count,
                validation::MAX_MULTIPART_FIELDS
            )));
        }

        if name == "content" {
            let filename = field
                .file_name()
                .ok_or_else(|| AppError::BadRequest("Missing filename in upload".to_string()))?
                .to_string();

            // Validate filename for security
            validate_filename(&filename)?;

            info!(filename = %filename, "Uploading PyPI package");

            // Only accept .whl and .tar.gz files
            if !package_utils::validate_file_extension(&filename, &[".whl", ".tar.gz"]) {
                warn!(filename = %filename, "Rejected file with invalid extension");
                return Err(AppError::BadRequest(
                    "Only .whl and .tar.gz files are allowed".to_string(),
                ));
            }

            // Read the data with size constraints to prevent memory exhaustion
            let data = field.bytes().await?;
            debug!(size = data.len(), "Read package data");

            // Use centralized validation for package uploads
            FileStreamValidator::validate_package_upload(&data, &filename, "PyPI")?;

            total_size += data.len() as u64;

            // Validate total multipart upload size
            validation::validate_multipart_limits(field_count, total_size, None).map_err(|e| {
                warn!(total_size = %total_size, field_count = %field_count, "Multipart limits exceeded");
                AppError::UploadError(format!("Multipart upload limits exceeded: {}", e))
            })?;

            // Calculate hash once during upload
            let hash = sha256_hash(&data);

            // Save the file using storage utility
            let file_path = pypi_dir.join(&filename);
            storage::save_file(&file_path, &data).await?;

            // Save the hash to a .meta file
            let meta_path = file_path.with_extension(format!(
                "{}.meta",
                file_path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .unwrap_or("")
            ));
            storage::save_file(meta_path, hash.as_bytes()).await?;

            info!(filename = %filename, size = data.len(), "PyPI package uploaded successfully");
            return Ok(axum::Json(SuccessResponse {
                message: "Upload successful".to_string(),
            }));
        } else {
            // For non-content fields, we still need to read and count them for size validation
            let field_data = field.bytes().await?;
            total_size += field_data.len() as u64;

            // Use centralized validation for total upload size
            FileStreamValidator::validate_total_upload_size(total_size, "PyPI")?;

            debug!(field_name = %name, size = field_data.len(), "Processed non-content field");
        }
    }

    warn!("No content field found in multipart upload");
    Err(AppError::BadRequest("No content field found".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum_test::{
        multipart::{MultipartForm, Part},
        TestServer,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use vm_package_server::{AppState, UpstreamClient, UpstreamConfig};

    fn create_pypi_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("pypi/packages")).unwrap();
        std::fs::create_dir_all(data_dir.join("pypi/simple")).unwrap();

        let upstream_config = UpstreamConfig::default();
        let config = Arc::new(vm_package_server::config::Config::default());
        let state = Arc::new(AppState {
            data_dir,
            server_addr: "http://127.0.0.1:3080".to_string(),
            upstream_client: Arc::new(UpstreamClient::new(upstream_config).unwrap()),
            config,
        });

        (state, temp_dir)
    }

    #[tokio::test]
    async fn test_upload_wheel_file() {
        let (state, _temp_dir) = create_pypi_test_state();
        let app = axum::Router::new()
            .route("/pypi/", axum::routing::post(upload_package))
            .with_state(state.clone());

        let server = TestServer::new(app).unwrap();

        let filename = "test_package-1.0.0-py3-none-any.whl";
        let content = b"fake wheel content";

        let part = Part::bytes(content.to_vec())
            .file_name(filename)
            .mime_type("application/octet-stream");
        let form = MultipartForm::new().add_part("content", part);

        let response = server.post("/pypi/").multipart(form).await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify file was saved
        let saved_path = state.data_dir.join("pypi/packages").join(filename);
        assert!(saved_path.exists());
        let saved_content = std::fs::read(saved_path).unwrap();
        assert_eq!(saved_content, content);
    }

    #[tokio::test]
    async fn test_upload_tar_gz_file() {
        let (state, _temp_dir) = create_pypi_test_state();
        let app = axum::Router::new()
            .route("/pypi/", axum::routing::post(upload_package))
            .with_state(state.clone());

        let server = TestServer::new(app).unwrap();

        let filename = "test-package-1.0.0.tar.gz";
        let content = b"fake tar.gz content";

        let part = Part::bytes(content.to_vec())
            .file_name(filename)
            .mime_type("application/octet-stream");
        let form = MultipartForm::new().add_part("content", part);

        let response = server.post("/pypi/").multipart(form).await;

        assert_eq!(response.status_code(), StatusCode::OK);

        // Verify file was saved
        let saved_path = state.data_dir.join("pypi/packages").join(filename);
        assert!(saved_path.exists());
    }

    #[tokio::test]
    async fn test_reject_invalid_file_extension() {
        let (state, _temp_dir) = create_pypi_test_state();
        let app = axum::Router::new()
            .route("/pypi/", axum::routing::post(upload_package))
            .with_state(state);

        let server = TestServer::new(app).unwrap();

        let filename = "test-package.txt";
        let content = b"invalid file content";

        let part = Part::bytes(content.to_vec())
            .file_name(filename)
            .mime_type("application/octet-stream");
        let form = MultipartForm::new().add_part("content", part);

        let response = server.post("/pypi/").multipart(form).await;

        assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_simple_index_shows_uploaded_package() {
        let (state, _temp_dir) = create_pypi_test_state();

        // Create a test package file
        let content = b"fake content";
        let package_file = state.data_dir.join("pypi/packages/testpackage-1.0.0.whl");
        std::fs::write(&package_file, content).unwrap();

        // Create corresponding .meta file with hash
        use vm_package_server::sha256_hash;
        let hash = sha256_hash(content);
        let meta_file = package_file.with_extension("whl.meta");
        std::fs::write(&meta_file, hash).unwrap();

        let app = axum::Router::new()
            .route("/pypi/simple/", axum::routing::get(simple_index))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server.get("/pypi/simple/").await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body = response.text();
        assert!(body.contains("testpackage"));
    }

    #[tokio::test]
    async fn test_package_index_shows_file_with_hash() {
        let (state, _temp_dir) = create_pypi_test_state();

        // Create a test package file
        let content = b"fake wheel content";
        let package_file = state.data_dir.join("pypi/packages/testpackage-1.0.0.whl");
        std::fs::write(&package_file, content).unwrap();

        // Create corresponding .meta file with hash
        use vm_package_server::sha256_hash;
        let hash = sha256_hash(content);
        let meta_file = package_file.with_extension("whl.meta");
        std::fs::write(&meta_file, hash).unwrap();

        let app = axum::Router::new()
            .route("/pypi/simple/{package}/", axum::routing::get(package_index))
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server.get("/pypi/simple/testpackage/").await;

        assert_eq!(response.status_code(), StatusCode::OK);
        let body = response.text();
        assert!(body.contains("testpackage-1.0.0.whl"));
        assert!(body.contains("sha256="));
    }

    #[tokio::test]
    async fn test_download_file() {
        let (state, _temp_dir) = create_pypi_test_state();

        // Create a test package file
        let content = b"test package content";
        let filename = "test-package-1.0.0.whl";
        let package_file = state.data_dir.join("pypi/packages").join(filename);
        std::fs::write(&package_file, content).unwrap();

        let app = axum::Router::new()
            .route(
                "/pypi/packages/{filename}",
                axum::routing::get(download_file),
            )
            .with_state(state);

        let server = TestServer::new(app).unwrap();
        let response = server.get(&format!("/pypi/packages/{}", filename)).await;

        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(response.as_bytes().to_vec(), content.to_vec());
    }
}
