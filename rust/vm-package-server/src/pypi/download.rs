use super::*;

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
/// ```text
/// GET /pypi/packages/package-name-1.0.0-py3-none-any.whl
/// ```
pub async fn download_file(
    AxumPath(filename): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    // Validate filename to prevent path traversal
    crate::validation::validate_filename(&filename)?;

    let data = storage::read_file(state.data_dir.join("pypi/packages").join(&filename)).await?;
    debug!(
        operation = "download",
        ecosystem = "python",
        filename = %filename,
        size = data.len(),
        "package artifact served"
    );
    Ok(data)
}

/// Proxies and persistently caches an immutable upstream PyPI artifact.
pub async fn download_upstream_file(
    AxumPath(path): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    let safe_path = validation::validate_safe_path(&path)
        .map_err(|error| AppError::BadRequest(format!("Invalid PyPI artifact path: {error}")))?;
    let filename = safe_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| AppError::BadRequest("PyPI artifact path has no filename".into()))?;
    crate::validation::validate_filename(filename)?;

    let cache_path = state.data_dir.join("cache/public/pypi").join(&safe_path);
    let upstream = Arc::clone(&state.upstream_client);
    let upstream_path = path.clone();
    let data = storage::read_through_cache(cache_path, move || async move {
        upstream
            .stream_pypi_file(&upstream_path)
            .await
            .map(|bytes| bytes.to_vec())
    })
    .await?;
    debug!(
        operation = "download",
        ecosystem = "python",
        source = "public",
        path = %path,
        size = data.len(),
        "package artifact served"
    );
    Ok(data)
}

/// Cache an immutable artifact fetched from the authoritative internal registry.
pub async fn download_internal_file(
    AxumPath(path): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Vec<u8>> {
    let safe_path = validation::validate_safe_path(&path)
        .map_err(|error| AppError::BadRequest(format!("Invalid PyPI artifact path: {error}")))?;
    let internal = state
        .internal_client
        .clone()
        .ok_or_else(|| AppError::NotFound("internal package source is not configured".into()))?;
    let cache_path = state.data_dir.join("cache/internal/pypi").join(&safe_path);
    let data = storage::read_through_cache(cache_path, move || async move {
        internal
            .pypi_artifact(&path)
            .await
            .map(|bytes| bytes.to_vec())
    })
    .await?;
    Ok(data)
}
