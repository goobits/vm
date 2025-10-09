//! # Input Validation: Package Manifests
//!
//! This module provides validation helpers for package manifests and other
//! package-related metadata, such as names and versions.

use crate::validation::error::ValidationError;
use crate::validation::limits::{
    MAX_METADATA_SIZE, MAX_PACKAGE_FILE_SIZE, MAX_PACKAGE_NAME_LENGTH, MAX_REQUEST_BODY_SIZE,
    MAX_VERSION_LENGTH,
};
use crate::validation::result::ValidationResult;

/// Validate package names according to ecosystem-specific rules.
///
/// This function validates package names for npm, PyPI, and Cargo ecosystems,
/// ensuring they conform to the respective naming conventions and don't contain
/// malicious characters.
///
/// # Arguments
///
/// * `name` - The package name to validate
/// * `ecosystem` - The package ecosystem ("npm", "pypi", "cargo")
///
/// # Returns
///
/// `Ok(String)` with the validated name, `Err(ValidationError)` if invalid
pub fn validate_package_name(name: &str, ecosystem: &str) -> ValidationResult<String> {
    // Common validations
    if name.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if name.len() > MAX_PACKAGE_NAME_LENGTH {
        return Err(ValidationError::TooLong {
            actual: name.len(),
            max: MAX_PACKAGE_NAME_LENGTH,
        });
    }

    // Check for null bytes and control characters
    if name.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    if name.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Basic character validation - allow letters, numbers, dots, hyphens, underscores
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    // Ecosystem-specific validations
    match ecosystem.to_lowercase().as_str() {
        "npm" => validate_npm_package_name(name),
        "pypi" => validate_pypi_package_name(name),
        "cargo" => validate_cargo_package_name(name),
        _ => Err(ValidationError::InvalidFormat {
            reason: format!("Unknown ecosystem: {}", ecosystem),
        }),
    }
}

/// Validate npm package names according to npm rules
fn validate_npm_package_name(name: &str) -> ValidationResult<String> {
    // npm specific rules
    if name.starts_with('.') || name.starts_with('_') {
        return Err(ValidationError::InvalidFormat {
            reason: "npm package names cannot start with . or _".to_string(),
        });
    }

    if name.to_lowercase() != name {
        return Err(ValidationError::InvalidFormat {
            reason: "npm package names must be lowercase".to_string(),
        });
    }

    // Check for URL-unsafe characters
    if name
        .chars()
        .any(|c| matches!(c, ' ' | '/' | '%' | '&' | '?' | '#'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    Ok(name.to_string())
}

/// Validate PyPI package names according to PEP 508
fn validate_pypi_package_name(name: &str) -> ValidationResult<String> {
    // PyPI allows letters, numbers, hyphens, periods, and underscores
    // Names are case-insensitive but normalized to lowercase with hyphens
    if name
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '.' | '_'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    // Cannot start with numbers
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(ValidationError::InvalidFormat {
            reason: "PyPI package names cannot start with numbers".to_string(),
        });
    }

    Ok(name.to_string())
}

/// Validate Cargo package names according to Cargo rules
fn validate_cargo_package_name(name: &str) -> ValidationResult<String> {
    // Cargo allows letters, numbers, hyphens, and underscores
    if name
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    // Cannot be empty or start with numbers
    if name.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        return Err(ValidationError::InvalidFormat {
            reason: "Cargo package names cannot start with numbers".to_string(),
        });
    }

    Ok(name.to_string())
}

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
pub fn validate_version(version: &str) -> ValidationResult<String> {
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
                "Payload size {} is smaller than expected minimum {}",
                payload_size, expected_min_size
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
    fn test_validate_package_name() {
        // npm tests
        assert!(validate_package_name("express", "npm").is_ok());
        assert!(validate_package_name("@scope/package", "npm").is_err()); // @ not in basic regex
        assert!(validate_package_name("Express", "npm").is_err()); // must be lowercase

        // PyPI tests
        assert!(validate_package_name("django", "pypi").is_ok());
        assert!(validate_package_name("django-rest-framework", "pypi").is_ok());
        assert!(validate_package_name("123invalid", "pypi").is_err()); // cannot start with number

        // Cargo tests
        assert!(validate_package_name("serde", "cargo").is_ok());
        assert!(validate_package_name("serde_json", "cargo").is_ok());
        assert!(validate_package_name("123invalid", "cargo").is_err()); // cannot start with number
    }

    #[test]
    fn test_validate_version() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("2.1.0-beta.1").is_ok());
        assert!(validate_version("").is_err());
        assert!(validate_version("version\0with\0nulls").is_err());

        // Test length limit
        let long_version = "1.0.0-".to_string() + &"a".repeat(MAX_VERSION_LENGTH);
        assert!(validate_version(&long_version).is_err());
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
