use std::sync::Arc;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::Html,
};
use tracing::{error, warn};

use crate::{AppError, AppResult, AppState};

/// Format file size in human-readable format
fn format_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = size as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    pypi_count: usize,
    npm_count: usize,
    cargo_count: usize,
    recent_packages: Vec<RecentPackage>,
    version: String,
    live_reload_script: String,
}

#[derive(Template)]
#[template(path = "upload.html")]
struct UploadTemplate {}

#[derive(Clone)]
struct RecentPackage {
    name: String,
    pkg_type: String,
    version: String,
}

#[derive(Template)]
#[template(path = "package_list.html")]
struct PackageListTemplate {
    pkg_type: String,
    packages: Vec<String>,
}

#[derive(Template)]
#[template(path = "pypi_detail.html")]
struct PyPiDetailTemplate {
    package_name: String,
    versions: Vec<PyPiVersion>,
}

#[derive(Clone)]
struct PyPiVersion {
    version: String,
    files: Vec<PackageFile>,
}

#[derive(Template)]
#[template(path = "npm_detail.html")]
struct NpmDetailTemplate {
    package_name: String,
    versions: Vec<NpmVersion>,
}

#[derive(Clone)]
struct NpmVersion {
    version: String,
    tarball: String,
    shasum: String,
    #[allow(dead_code)]
    size: u64,
    size_formatted: String,
}

#[derive(Template)]
#[template(path = "cargo_detail.html")]
struct CargoDetailTemplate {
    package_name: String,
    versions: Vec<CargoVersion>,
}

#[derive(Clone)]
struct CargoVersion {
    version: String,
    checksum: String,
    #[allow(dead_code)]
    size: u64,
    size_formatted: String,
}

/// Represents a package file with its hash for verification
#[derive(Clone)]
pub struct PackageFile {
    pub filename: String,
    pub sha256: String,
    #[allow(dead_code)]
    pub size: u64,
    pub size_formatted: String,
}

/// Render the home page with package statistics and recent packages
pub async fn home(State(state): State<Arc<AppState>>) -> AppResult<Html<String>> {
    // Fetch package counts with fallback to 0 on error
    #[allow(deprecated)]
    let pypi_count = match crate::pypi::count_packages(&state).await {
        Ok(count) => count,
        Err(e) => {
            warn!("Failed to get PyPI package count: {}", e);
            0
        }
    };

    let npm_count = match crate::npm::count_packages(&state).await {
        Ok(count) => count,
        Err(e) => {
            warn!("Failed to get npm package count: {}", e);
            0
        }
    };

    let cargo_count = match crate::cargo::count_crates(&state).await {
        Ok(count) => count,
        Err(e) => {
            warn!("Failed to get Cargo crate count: {}", e);
            0
        }
    };

    // Fetch recent packages with fallback to empty list on error
    let recent_packages = match get_recent_packages(&state).await {
        Ok(packages) => packages,
        Err(e) => {
            warn!("Failed to get recent packages: {}", e);
            Vec::new() // Fallback to empty list for graceful degradation
        }
    };

    // Include live reload script only in debug builds
    let live_reload_script = if cfg!(debug_assertions) {
        crate::live_reload::inject_reload_script().to_string()
    } else {
        String::new()
    };

    let template = IndexTemplate {
        pypi_count,
        npm_count,
        cargo_count,
        recent_packages,
        version: env!("CARGO_PKG_VERSION").to_string(),
        live_reload_script,
    };

    Ok(Html(template.render().map_err(|e| {
        error!("Template render error: {}", e);
        AppError::Anyhow(anyhow::anyhow!("Template render error: {}", e))
    })?))
}

/// List all packages of a given type (npm, pypi, cargo)
pub async fn list_packages(
    Path(pkg_type): Path<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Html<String>> {
    let packages = match pkg_type.as_str() {
        #[allow(deprecated)]
        "pypi" => crate::pypi::list_all_packages(&state).await?,
        "npm" => crate::npm::list_all_packages(&state).await?,
        "cargo" => crate::cargo::list_all_crates(&state).await?,
        _ => return Err(AppError::NotFound("Invalid package type".to_string())),
    };

    let template = PackageListTemplate { pkg_type, packages };

    Ok(Html(template.render().map_err(|e| {
        error!("Template render error: {}", e);
        AppError::Anyhow(anyhow::anyhow!("Template render error: {}", e))
    })?))
}

/// Show detailed information for a specific PyPI package
pub async fn pypi_package_detail(
    Path(pkg_name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Html<String>> {
    let versions = crate::pypi::get_package_versions(&state, &pkg_name).await?;

    let template = PyPiDetailTemplate {
        package_name: pkg_name,
        versions: versions
            .into_iter()
            .map(|(version, files)| PyPiVersion {
                version,
                files: files
                    .into_iter()
                    .map(|(filename, sha256, size)| PackageFile {
                        filename,
                        sha256,
                        size,
                        size_formatted: format_size(size),
                    })
                    .collect(),
            })
            .collect(),
    };

    Ok(Html(template.render().map_err(|e| {
        error!("Template render error: {}", e);
        AppError::Anyhow(anyhow::anyhow!("Template render error: {}", e))
    })?))
}

/// Show detailed information for a specific npm package
pub async fn npm_package_detail(
    Path(pkg_name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Html<String>> {
    let versions = crate::npm::get_package_versions(&state, &pkg_name).await?;

    let template = NpmDetailTemplate {
        package_name: pkg_name,
        versions: versions
            .into_iter()
            .map(|(version, tarball, shasum, size)| NpmVersion {
                version,
                tarball,
                shasum,
                size,
                size_formatted: format_size(size),
            })
            .collect(),
    };

    Ok(Html(template.render().map_err(|e| {
        error!("Template render error: {}", e);
        AppError::Anyhow(anyhow::anyhow!("Template render error: {}", e))
    })?))
}

/// Show detailed information for a specific Cargo crate
pub async fn cargo_package_detail(
    Path(pkg_name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Html<String>> {
    let versions = crate::cargo::get_crate_versions(&state, &pkg_name).await?;

    let template = CargoDetailTemplate {
        package_name: pkg_name,
        versions: versions
            .into_iter()
            .map(|(version, checksum, size)| CargoVersion {
                version,
                checksum,
                size,
                size_formatted: format_size(size),
            })
            .collect(),
    };

    Ok(Html(template.render().map_err(|e| {
        error!("Template render error: {}", e);
        AppError::Anyhow(anyhow::anyhow!("Template render error: {}", e))
    })?))
}

/// Render the upload page
pub async fn upload_page() -> AppResult<Html<String>> {
    let template = UploadTemplate {};

    Ok(Html(template.render().map_err(|e| {
        error!("Template render error: {}", e);
        AppError::Anyhow(anyhow::anyhow!("Template render error: {}", e))
    })?))
}

async fn get_recent_packages(state: &AppState) -> AppResult<Vec<RecentPackage>> {
    let mut recent = Vec::new();

    // Fetch recent PyPI packages with proper error handling
    match crate::pypi::get_recent_packages(state, 5).await {
        Ok(pypi_packages) => {
            for (name, version) in pypi_packages {
                recent.push(RecentPackage {
                    name,
                    pkg_type: "pypi".to_string(),
                    version,
                });
            }
        }
        Err(e) => {
            warn!("Failed to fetch recent PyPI packages: {}", e);
            // Continue without PyPI packages - graceful degradation
        }
    }

    // Fetch recent npm packages with proper error handling
    match crate::npm::get_recent_packages(state, 5).await {
        Ok(npm_packages) => {
            for (name, version) in npm_packages {
                recent.push(RecentPackage {
                    name,
                    pkg_type: "npm".to_string(),
                    version,
                });
            }
        }
        Err(e) => {
            warn!("Failed to fetch recent npm packages: {}", e);
            // Continue without npm packages - graceful degradation
        }
    }

    // Fetch recent Cargo crates with proper error handling
    match crate::cargo::get_recent_crates(state, 5).await {
        Ok(cargo_crates) => {
            for (name, version) in cargo_crates {
                recent.push(RecentPackage {
                    name,
                    pkg_type: "cargo".to_string(),
                    version,
                });
            }
        }
        Err(e) => {
            warn!("Failed to fetch recent Cargo crates: {}", e);
            // Continue without Cargo crates - graceful degradation
        }
    }

    recent.truncate(10);
    Ok(recent)
}
