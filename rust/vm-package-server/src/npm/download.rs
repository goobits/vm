use super::*;

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
    crate::validation::validate_filename(&filename)?;

    let local_path = state.data_dir.join("npm/tarballs").join(&filename);
    let fallback_state = Arc::clone(&state);
    let fallback_package = package.clone();
    let fallback_filename = filename.clone();
    let data = storage::read_local_or_else(local_path, move || async move {
        let source = fallback_state
            .resolver
            .resolve_missing(
                vm_packages::PackageEcosystem::Npm,
                &fallback_package,
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
            .join("npm")
            .join(sha256_hex(fallback_package.as_bytes()))
            .join(&fallback_filename);
        let tarball_url = format!("/{fallback_package}/-/{fallback_filename}");
        storage::read_through_cache(cache_path, move || async move {
            let bytes = match source {
                vm_packages::ResolutionSource::InternalRegistry => {
                    fallback_state
                        .internal_client
                        .as_ref()
                        .expect("resolver only selects a configured internal registry")
                        .npm_tarball(&fallback_package, &fallback_filename)
                        .await?
                }
                vm_packages::ResolutionSource::PublicUpstream => {
                    fallback_state
                        .upstream_client
                        .stream_npm_tarball(&tarball_url)
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
        ecosystem = "npm",
        package = %package,
        filename = %filename,
        size = data.len(),
        "package artifact served"
    );
    Ok(data)
}
