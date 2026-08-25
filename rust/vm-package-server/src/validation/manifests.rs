//! # Input Validation: Package Manifests
//!
//! This module provides validation helpers for package manifests and other
//! package-related metadata, such as names and versions.

use crate::validation::error::ValidationError;
use crate::validation::limits::{
    MAX_METADATA_SIZE, MAX_PACKAGE_FILE_SIZE, MAX_REQUEST_BODY_SIZE, MAX_VERSION_LENGTH,
};
use crate::validation::result::ValidationResult;

/// Validate version strings according to semantic versioning principles.
///
/// This function validates version strings to ensure they follow a reasonable
/// format and don't contain malicious characters.
///
/// # Arguments
///
/// * `version` - The version string to validate
///
/// # Returns
///
/// `Ok(String)` with the validated version, `Err(ValidationError)` if invalid
pub fn validate_registry_version(version: &str) -> ValidationResult<String> {
    if version.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if version.len() > MAX_VERSION_LENGTH {
        return Err(ValidationError::TooLong {
            actual: version.len(),
            max: MAX_VERSION_LENGTH,
        });
    }

    // Check for null bytes and control characters
    if version.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    if version.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Validate version format - allow letters, numbers, dots, hyphens, underscores, plus
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: version.to_string(),
        });
    }

    Ok(version.to_string())
}

/// Validate cargo upload payload structure and size limits.
///
/// This function validates the binary structure of cargo upload payloads
/// to ensure they conform to expected format and size limits.
///
/// # Arguments
///
/// * `payload_size` - Total size of the upload payload
/// * `metadata_size` - Size of the metadata portion
/// * `crate_size` - Size of the crate file portion
///
/// # Returns
///
/// `Ok(())` if valid structure and sizes, `Err(ValidationError)` otherwise
pub fn validate_cargo_upload_structure(
    payload_size: usize,
    metadata_size: usize,
    crate_size: usize,
) -> ValidationResult<()> {
    // Validate total payload size
    if payload_size > MAX_REQUEST_BODY_SIZE {
        return Err(ValidationError::FileTooLarge {
            actual: payload_size as u64,
            max: MAX_REQUEST_BODY_SIZE as u64,
        });
    }

    // Validate metadata size
    if metadata_size > MAX_METADATA_SIZE {
        return Err(ValidationError::FileTooLarge {
            actual: metadata_size as u64,
            max: MAX_METADATA_SIZE as u64,
        });
    }

    // Validate crate file size
    if crate_size > MAX_PACKAGE_FILE_SIZE as usize {
        return Err(ValidationError::FileTooLarge {
            actual: crate_size as u64,
            max: MAX_PACKAGE_FILE_SIZE,
        });
    }

    // Validate structure: payload should be approximately metadata + crate + headers
    let expected_min_size = metadata_size + crate_size + 8; // 8 bytes for length headers
    if payload_size < expected_min_size {
        return Err(ValidationError::InvalidFormat {
            reason: format!(
                "Payload size {payload_size} is smaller than expected minimum {expected_min_size}"
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::limits::MAX_VERSION_LENGTH;

    #[test]
    fn test_validate_registry_version() {
        assert!(validate_registry_version("1.0.0").is_ok());
        assert!(validate_registry_version("2.1.0-beta.1").is_ok());
        assert!(validate_registry_version("").is_err());
        assert!(validate_registry_version("version\0with\0nulls").is_err());

        // Test length limit
        let long_version = "1.0.0-".to_string() + &"a".repeat(MAX_VERSION_LENGTH);
        assert!(validate_registry_version(&long_version).is_err());
    }

    #[test]
    fn test_validate_cargo_upload_structure() {
        // Valid structure
        let metadata_size = 1024;
        let crate_size = 10 * 1024 * 1024; // 10MB
        let payload_size = metadata_size + crate_size + 8;
        assert!(validate_cargo_upload_structure(payload_size, metadata_size, crate_size).is_ok());

        // Payload too large
        assert!(validate_cargo_upload_structure(MAX_REQUEST_BODY_SIZE + 1, 1024, 1024).is_err());

        // Metadata too large
        assert!(
            validate_cargo_upload_structure(1024 + 8 + 1024, MAX_METADATA_SIZE + 1, 1024).is_err()
        );

        // Crate file too large
        let large_crate_size = MAX_PACKAGE_FILE_SIZE as usize + 1;
        assert!(validate_cargo_upload_structure(
            large_crate_size + 1024 + 8,
            1024,
            large_crate_size
        )
        .is_err());

        // Payload smaller than expected
        assert!(validate_cargo_upload_structure(100, 1024, 1024).is_err());
    }
}
