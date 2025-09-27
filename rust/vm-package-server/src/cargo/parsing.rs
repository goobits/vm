//! Cargo package upload parsing and validation
//!
//! This module handles parsing of Cargo publish payloads, extracting metadata
//! and .crate file data from the binary upload format.

use super::CrateMetadata;
use crate::validation_utils::FileStreamValidator;
use crate::{validation, AppError, AppResult};
use serde_json::{json, Value};
use tracing::{debug, warn};

/// Parse and validate crate upload payload with comprehensive size and structure validation
/// Returns (CrateMetadata, crate_data)
pub fn parse_crate_upload(body: axum::body::Bytes) -> AppResult<(CrateMetadata, Vec<u8>)> {
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
