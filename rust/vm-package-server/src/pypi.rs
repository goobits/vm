use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    extract::{Multipart, Path as AxumPath, State},
    http::HeaderMap,
    response::Response,
};
use tracing::{debug, info, warn};
use vm_packages::{sha256_hex, PackageEcosystem, PackageIdentity};

mod download;
mod html;
mod upload;
pub use download::{download_file, download_internal_file, download_upstream_file};
pub use upload::upload_package;

use crate::validation;
use crate::{package_utils, storage, AppError, AppResult, AppState, SuccessResponse};

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
pub async fn simple_index(State(state): State<Arc<AppState>>) -> AppResult<Response> {
    let pypi_dir = state.data_dir.join("pypi/packages");

    let mut packages = std::collections::HashSet::new();

    for path in list_package_files(&pypi_dir).await? {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(pkg_name) = crate::utils::extract_pypi_package_name(name) {
                packages.insert(pkg_name);
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
        let href = html::escape(&format!("{}/", html::path_segment(&package)));
        let package = html::escape(&package);
        html.push_str(&format!(r#"    <a href="{href}">{package}</a><br/>"#));
    }

    html.push_str("  </body>\n</html>");
    Ok(html::response(html))
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
    headers: HeaderMap,
) -> AppResult<Response> {
    let normalized_package = PackageIdentity::new(PackageEcosystem::Python, &package)
        .map_err(|error| AppError::BadRequest(error.to_string()))?
        .name;
    let pypi_dir = state.data_dir.join("pypi/packages");
    let mut files = Vec::new();

    for path in list_package_files(&pypi_dir).await? {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(pkg_name) = crate::utils::extract_pypi_package_name(name) {
                if pkg_name == normalized_package {
                    let meta_path = path.with_extension(format!(
                        "{}.meta",
                        path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
                    ));
                    match storage::read_file_string(&meta_path).await {
                        Ok(hash) => {
                            files.push((name.to_string(), hash.trim().to_string()));
                        }
                        Err(_) => {
                            warn!(
                                operation = "build_index",
                                ecosystem = "python",
                                package = %package,
                                filename = %name,
                                "package artifact metadata missing"
                            );
                        }
                    }
                }
            }
        }
    }

    // If no local files found, try upstream PyPI
    if files.is_empty() {
        let source = state
            .resolver
            .resolve_missing(
                vm_packages::PackageEcosystem::Python,
                &normalized_package,
                state.internal_client.is_some(),
            )
            .await?;
        debug!(
            operation = "resolve_index",
            ecosystem = "python",
            package = %package,
            source = ?source,
            "package index cache miss"
        );
        let cache_scope = match source {
            vm_packages::ResolutionSource::InternalRegistry => "internal",
            vm_packages::ResolutionSource::PublicUpstream => "public",
            _ => unreachable!("local releases are checked before source resolution"),
        };
        let cache_path = state
            .data_dir
            .join("cache")
            .join(cache_scope)
            .join("pypi/simple")
            .join(format!("{normalized_package}.html"));
        let internal = state.internal_client.clone();
        let upstream = Arc::clone(&state.upstream_client);
        let resolved_package = normalized_package.clone();
        let html = storage::read_refreshing_cache(
            cache_path,
            storage::METADATA_CACHE_TTL,
            move || async move {
                let html = match source {
                    vm_packages::ResolutionSource::InternalRegistry => {
                        internal
                            .expect("resolver only selects a configured internal registry")
                            .pypi_index(&resolved_package)
                            .await?
                    }
                    vm_packages::ResolutionSource::PublicUpstream => {
                        upstream.fetch_pypi_simple(&resolved_package).await?
                    }
                    _ => unreachable!("local releases are checked before source resolution"),
                };
                Ok(html.into_bytes())
            },
        )
        .await?;
        let html = String::from_utf8(html).map_err(|error| AppError::Utf8(error.utf8_error()))?;
        let html = match source {
            vm_packages::ResolutionSource::InternalRegistry => rewrite_internal_links(
                html,
                state
                    .internal_client
                    .as_ref()
                    .expect("resolver only selects a configured internal registry")
                    .gateway(),
                &state.public_base_url(&headers),
            ),
            vm_packages::ResolutionSource::PublicUpstream => {
                rewrite_upstream_links(html, &state.public_base_url(&headers))
            }
            _ => unreachable!("local releases are checked before source resolution"),
        };
        debug!(
            operation = "resolve_index",
            ecosystem = "python",
            package = %package,
            source = ?source,
            "package index resolved"
        );
        return Ok(html::response(html));
    }

    let package = html::escape(&normalized_package);
    let mut html = format!(
        r#"<!DOCTYPE html>
<html>
  <head><title>Links for {package}</title></head>
  <body>
    <h1>Links for {package}</h1>
"#
    );

    for (filename, hash) in files {
        html.push_str(&html::artifact_link(&filename, &hash));
    }

    html.push_str("  </body>\n</html>");
    Ok(html::response(html))
}

fn rewrite_upstream_links(html: String, public_base_url: &str) -> String {
    html.replace(
        "https://files.pythonhosted.org/packages/",
        &html::escape(&format!("{public_base_url}/pypi/upstream/")),
    )
}

fn rewrite_internal_links(html: String, internal_gateway: &str, public_base_url: &str) -> String {
    html.replace("../../packages/", "../../internal/packages/")
        .replace(
            &format!("{}/pypi/", internal_gateway.trim_end_matches('/')),
            &html::escape(&format!("{public_base_url}/pypi/internal/")),
        )
}

#[cfg(test)]
mod tests;
