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
    super::validate_crate_name(crate_name)?;
    validation::validate_registry_version(version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version '{version}': {e}")))?;

    let filename = format!("{crate_name}-{version}.crate");

    // Validate the constructed filename path
    validation::validate_safe_path(&filename).map_err(|e| {
        AppError::BadRequest(format!("Generated unsafe filename '{filename}': {e}"))
    })?;

    let crate_path = data_dir.join("cargo/crates").join(&filename);

    storage::save_immutable(&crate_path, data).await?;

    Ok(crate_path)
}
