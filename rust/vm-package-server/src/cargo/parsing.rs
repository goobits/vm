//! Cargo package upload parsing and validation
//!
//! This module handles parsing of Cargo publish payloads, extracting metadata
//! and .crate file data from the binary upload format.

use super::CrateMetadata;
use crate::{validation, AppError, AppResult};
use serde_json::{json, Value};
use tracing::debug;

/// Parse and validate crate upload payload with comprehensive size and structure validation
/// Returns (CrateMetadata, crate_data)
pub fn parse_crate_upload(body: axum::body::Bytes) -> AppResult<(CrateMetadata, Vec<u8>)> {
    let data = &body[..];
    let payload_size = data.len();

    // Validate minimum payload size for headers
    if payload_size < 8 {
        return Err(AppError::UploadError(
            "Payload too small - missing required headers".to_string(),
        ));
    }

    // Validate total payload size using our centralized validation
    validation::validate_file_size(
        payload_size as u64,
        Some(validation::MAX_REQUEST_BODY_SIZE as u64),
    )
    .map_err(|e| AppError::UploadError(format!("Payload too large: {e}")))?;

    // Parse: 4-byte metadata length + JSON metadata + 4-byte crate length + .crate file
    let metadata_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;

    // Validate metadata size
    if metadata_len > validation::MAX_METADATA_SIZE {
        return Err(AppError::UploadError(format!(
            "Metadata section too large: {} bytes (max: {} bytes)",
            metadata_len,
            validation::MAX_METADATA_SIZE
        )));
    }

    if payload_size < 4 + metadata_len + 4 {
        return Err(AppError::UploadError(
            "Insufficient data for metadata section".to_string(),
        ));
    }

    // Extract and validate JSON metadata
    let metadata_bytes = &data[4..4 + metadata_len];
    let metadata: Value = serde_json::from_slice(metadata_bytes)
        .map_err(|e| AppError::UploadError(format!("Invalid metadata JSON: {e}")))?;

    let crate_name = metadata["name"].as_str().ok_or_else(|| {
        AppError::UploadError("'name' field missing or not a string in metadata".to_string())
    })?;
    let version = metadata["vers"].as_str().ok_or_else(|| {
        AppError::UploadError("'vers' field missing or not a string in metadata".to_string())
    })?;

    // Validate crate name and version early
    super::validate_crate_name(crate_name)
        .map_err(|error| AppError::BadRequest(format!("Invalid crate name: {error}")))?;
    validation::validate_registry_version(version)
        .map_err(|e| AppError::BadRequest(format!("Invalid version: {e}")))?;

    // Extract .crate file length
    let crate_len_offset = 4 + metadata_len;
    if payload_size < crate_len_offset + 4 {
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
    validation::validate_total_upload_size(crate_len as u64, "Cargo")?;

    let crate_data_offset = crate_len_offset + 4;
    if payload_size < crate_data_offset + crate_len {
        return Err(AppError::UploadError(
            "Insufficient data for crate file".to_string(),
        ));
    }

    // Validate the overall structure using our centralized validation
    validation::validate_cargo_upload_structure(payload_size, metadata_len, crate_len)
        .map_err(|e| AppError::UploadError(format!("Invalid upload structure: {e}")))?;

    // Extract .crate file
    let crate_data = &data[crate_data_offset..crate_data_offset + crate_len];
    debug!(crate_size = crate_data.len(), "Extracted crate file data");

    // Use centralized validation for extracted crate package
    validation::validate_package_upload(crate_data, "crate", "Cargo")?;

    let crate_metadata = CrateMetadata {
        name: crate_name.to_string(),
        version: version.to_string(),
        // Safe: unwrap_or provides sensible defaults for optional metadata fields
        // deps defaults to empty array, features defaults to empty object
        deps: index_dependencies(metadata.get("deps"))?,
        features: metadata.get("features").unwrap_or(&json!({})).clone(),
        links: metadata
            .get("links")
            .and_then(Value::as_str)
            .map(str::to_owned),
        rust_version: metadata
            .get("rust_version")
            .and_then(Value::as_str)
            .map(str::to_owned),
    };

    Ok((crate_metadata, crate_data.to_vec()))
}

// Cargo's upload schema uses version_req and the original package name;
// its index schema uses req and the dependency's name in the manifest.
pub(super) fn index_dependencies(dependencies: Option<&Value>) -> AppResult<Value> {
    let Some(dependencies) = dependencies.filter(|value| !value.is_null()) else {
        return Ok(json!([]));
    };
    let dependencies = dependencies
        .as_array()
        .ok_or_else(|| AppError::UploadError("'deps' must be an array".into()))?;
    let mut entries = Vec::with_capacity(dependencies.len());
    for dependency in dependencies {
        let name = dependency
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::UploadError("dependency name is missing".into()))?;
        let requirement = dependency
            .get("version_req")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::UploadError("dependency version_req is missing".into()))?;
        let alias = dependency
            .get("explicit_name_in_toml")
            .and_then(Value::as_str);
        entries.push(json!({
            "name": alias.unwrap_or(name),
            "req": requirement,
            "features": dependency.get("features").cloned().unwrap_or_else(|| json!([])),
            "optional": dependency.get("optional").and_then(Value::as_bool).unwrap_or(false),
            "default_features": dependency.get("default_features").and_then(Value::as_bool).unwrap_or(true),
            "target": dependency.get("target"),
            "kind": dependency.get("kind").and_then(Value::as_str).unwrap_or("normal"),
            "registry": dependency.get("registry"),
            "package": alias.map(|_| name),
        }));
    }
    Ok(Value::Array(entries))
}
