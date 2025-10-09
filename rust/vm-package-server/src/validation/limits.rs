//! # Input Validation: Size Limits & Thresholds
//!
//! This module defines constants for various size and length limits used
//! throughout the package server to prevent resource exhaustion attacks.

use crate::validation::error::ValidationError;
use crate::validation::result::ValidationResult;

/// Maximum allowed file size for uploads (100 MB)
pub const MAX_UPLOAD_SIZE: u64 = 100 * 1024 * 1024;

/// Maximum allowed request body size (120 MB) - allows overhead for multipart encoding
pub const MAX_REQUEST_BODY_SIZE: usize = 120 * 1024 * 1024;

/// Maximum allowed package file size (80 MB) - actual package content limit
pub const MAX_PACKAGE_FILE_SIZE: u64 = 80 * 1024 * 1024;

/// Maximum allowed base64 encoded size (110 MB) - to prevent base64 bombs
pub const MAX_BASE64_ENCODED_SIZE: usize = 110 * 1024 * 1024;

/// Maximum allowed decoded base64 size (80 MB) - actual data after decoding
pub const MAX_BASE64_DECODED_SIZE: usize = 80 * 1024 * 1024;

/// Maximum allowed metadata size for package manifests (1 MB)
pub const MAX_METADATA_SIZE: usize = 1024 * 1024;

/// Maximum allowed number of multipart fields
pub const MAX_MULTIPART_FIELDS: usize = 10;

/// Memory threshold for streaming vs loading into memory (10 MB)
pub const MEMORY_THRESHOLD: usize = 10 * 1024 * 1024;

/// Maximum allowed package name length
pub const MAX_PACKAGE_NAME_LENGTH: usize = 214;

/// Maximum allowed version string length
pub const MAX_VERSION_LENGTH: usize = 64;

/// Maximum allowed description length
pub const MAX_DESCRIPTION_LENGTH: usize = 4096;

/// Maximum allowed filename length
pub const MAX_FILENAME_LENGTH: usize = 255;

/// Maximum allowed path depth for relative paths
pub const MAX_PATH_DEPTH: usize = 10;

/// Validate file size against limits.
///
/// This function checks if a file size is within acceptable limits to prevent
/// resource exhaustion attacks.
///
/// # Arguments
///
/// * `size` - The file size in bytes
/// * `max_size` - Optional custom maximum size (defaults to MAX_UPLOAD_SIZE)
///
/// # Returns
///
/// `Ok(())` if size is acceptable, `Err(ValidationError)` if too large
pub fn validate_file_size(size: u64, max_size: Option<u64>) -> ValidationResult<()> {
    let limit = max_size.unwrap_or(MAX_UPLOAD_SIZE);

    if size > limit {
        return Err(ValidationError::FileTooLarge {
            actual: size,
            max: limit,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_file_size() {
        assert!(validate_file_size(1024, None).is_ok());
        assert!(validate_file_size(MAX_UPLOAD_SIZE, None).is_ok());
        assert!(validate_file_size(MAX_UPLOAD_SIZE + 1, None).is_err());
        assert!(validate_file_size(1024, Some(512)).is_err());
    }
}