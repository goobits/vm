//! Cargo file storage operations
//!
//! This module handles saving and retrieving Cargo crate files from the local filesystem.

use crate::{storage, validation, AppError, AppResult};
use std::path::PathBuf;

/// Save crate file to the appropriate directory
pub async fn save_crate_file(
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
