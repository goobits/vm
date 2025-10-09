//! # Input Validation: HTTP
//!
//! This module provides validation helpers for HTTP-related data, such as
//! hostnames, base64 encoded data, and multipart uploads.

use crate::validation::error::ValidationError;
use crate::validation::limits::{
    MAX_BASE64_DECODED_SIZE, MAX_BASE64_ENCODED_SIZE, MAX_MULTIPART_FIELDS, MAX_UPLOAD_SIZE,
};
use crate::validation::result::ValidationResult;
use once_cell::sync::Lazy;
use regex::Regex;

/// Regex for validating hostnames (RFC 1123 compliant)
static HOSTNAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$")
        .expect("Hostname regex should compile - this is a static RFC 1123 pattern")
});

/// Validate and sanitize hostnames for network operations.
///
/// This function validates hostnames according to RFC 1123 to ensure they
/// are safe for network operations and don't contain injection patterns.
///
/// # Arguments
///
/// * `hostname` - The hostname to validate
///
/// # Returns
///
/// `Ok(String)` with the validated hostname, `Err(ValidationError)` if invalid
pub fn validate_hostname(hostname: &str) -> ValidationResult<String> {
    if hostname.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if hostname.len() > 253 {
        return Err(ValidationError::TooLong {
            actual: hostname.len(),
            max: 253,
        });
    }

    // Check for null bytes and control characters
    if hostname.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    if hostname.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Validate hostname format
    if !HOSTNAME_REGEX.is_match(hostname) {
        return Err(ValidationError::InvalidCharacters {
            input: hostname.to_string(),
        });
    }

    // Additional checks
    if hostname.starts_with('-') || hostname.ends_with('-') {
        return Err(ValidationError::InvalidFormat {
            reason: "Hostnames cannot start or end with hyphens".to_string(),
        });
    }

    if hostname.starts_with('.') || hostname.ends_with('.') {
        return Err(ValidationError::InvalidFormat {
            reason: "Hostnames cannot start or end with dots".to_string(),
        });
    }

    Ok(hostname.to_string())
}

/// Validate base64 encoded data size before decoding to prevent base64 bombs.
///
/// This function checks the size of base64 encoded data before attempting to decode
/// it, preventing memory exhaustion attacks from maliciously crafted base64 data.
///
/// # Arguments
///
/// * `encoded_data` - The base64 encoded string to validate
/// * `max_encoded_size` - Optional maximum encoded size (defaults to MAX_BASE64_ENCODED_SIZE)
/// * `max_decoded_size` - Optional maximum decoded size (defaults to MAX_BASE64_DECODED_SIZE)
///
/// # Returns
///
/// `Ok(())` if the encoded data is within limits, `Err(ValidationError)` otherwise
pub fn validate_base64_size(
    encoded_data: &str,
    max_encoded_size: Option<usize>,
    max_decoded_size: Option<usize>,
) -> ValidationResult<()> {
    let encoded_limit = max_encoded_size.unwrap_or(MAX_BASE64_ENCODED_SIZE);
    let decoded_limit = max_decoded_size.unwrap_or(MAX_BASE64_DECODED_SIZE);

    // Check encoded size first
    if encoded_data.len() > encoded_limit {
        return Err(ValidationError::FileTooLarge {
            actual: encoded_data.len() as u64,
            max: encoded_limit as u64,
        });
    }

    // Estimate decoded size (base64 encodes 3 bytes as 4 characters)
    let estimated_decoded_size = (encoded_data.len() * 3) / 4;
    if estimated_decoded_size > decoded_limit {
        return Err(ValidationError::FileTooLarge {
            actual: estimated_decoded_size as u64,
            max: decoded_limit as u64,
        });
    }

    Ok(())
}

/// Validate that base64 data contains only valid characters.
///
/// This function ensures base64 data contains only valid base64 characters
/// to prevent injection attacks and invalid data processing.
///
/// # Arguments
///
/// * `data` - The base64 string to validate
///
/// # Returns
///
/// `Ok(())` if valid base64 characters, `Err(ValidationError)` otherwise
pub fn validate_base64_characters(data: &str) -> ValidationResult<()> {
    if data.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    // Check for valid base64 characters only
    if !data
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    {
        return Err(ValidationError::InvalidCharacters {
            input: "Invalid base64 characters detected".to_string(),
        });
    }

    Ok(())
}

/// Validate multipart upload limits to prevent memory exhaustion.
///
/// This function validates multipart upload parameters including field count,
/// field sizes, and total payload size to prevent DoS attacks.
///
/// # Arguments
///
/// * `field_count` - Number of multipart fields
/// * `total_size` - Total size of all fields combined
/// * `max_fields` - Optional maximum field count (defaults to MAX_MULTIPART_FIELDS)
///
/// # Returns
///
/// `Ok(())` if within limits, `Err(ValidationError)` otherwise
pub fn validate_multipart_limits(
    field_count: usize,
    total_size: u64,
    max_fields: Option<usize>,
) -> ValidationResult<()> {
    let field_limit = max_fields.unwrap_or(MAX_MULTIPART_FIELDS);

    if field_count > field_limit {
        return Err(ValidationError::TooLong {
            actual: field_count,
            max: field_limit,
        });
    }

    if total_size > MAX_UPLOAD_SIZE {
        return Err(ValidationError::FileTooLarge {
            actual: total_size,
            max: MAX_UPLOAD_SIZE,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validation::limits::{
        MAX_BASE64_DECODED_SIZE, MAX_BASE64_ENCODED_SIZE, MAX_MULTIPART_FIELDS, MAX_UPLOAD_SIZE,
    };

    #[test]
    fn test_validate_hostname() {
        assert!(validate_hostname("example.com").is_ok());
        assert!(validate_hostname("sub.example.com").is_ok());
        assert!(validate_hostname("localhost").is_ok());

        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("-invalid.com").is_err());
        assert!(validate_hostname("invalid-.com").is_err());
        assert!(validate_hostname(".invalid.com").is_err());
        assert!(validate_hostname("invalid.com.").is_err());
    }

    #[test]
    fn test_validate_base64_size() {
        // Valid base64 data within limits
        let small_base64 = "SGVsbG8gV29ybGQ="; // "Hello World"
        assert!(validate_base64_size(small_base64, None, None).is_ok());

        // Test with custom limits
        assert!(validate_base64_size(small_base64, Some(100), Some(50)).is_ok());

        // Test encoded size limit
        let large_encoded = "a".repeat(MAX_BASE64_ENCODED_SIZE + 1);
        assert!(validate_base64_size(&large_encoded, None, None).is_err());

        // Test estimated decoded size limit (create string that will decode to more than the limit)
        // Base64 encodes 3 bytes as 4 characters, so we need 4/3 * limit + padding to exceed it
        let size_that_decodes_too_large = "a".repeat(((MAX_BASE64_DECODED_SIZE * 4) / 3) + 100);
        assert!(validate_base64_size(&size_that_decodes_too_large, None, None).is_err());
    }

    #[test]
    fn test_validate_base64_characters() {
        // Valid base64
        assert!(validate_base64_characters("SGVsbG8gV29ybGQ=").is_ok());
        assert!(validate_base64_characters("YWJjZGVmZw==").is_ok());

        // Invalid characters
        assert!(validate_base64_characters("Hello World!").is_err());
        assert!(validate_base64_characters("abc@def").is_err());
        assert!(validate_base64_characters("abc def").is_err());

        // Empty string
        assert!(validate_base64_characters("").is_err());
    }

    #[test]
    fn test_validate_multipart_limits() {
        // Valid limits
        assert!(validate_multipart_limits(5, 1024 * 1024, None).is_ok());

        // Too many fields
        assert!(validate_multipart_limits(MAX_MULTIPART_FIELDS + 1, 1024, None).is_err());

        // Total size too large
        assert!(validate_multipart_limits(5, MAX_UPLOAD_SIZE + 1, None).is_err());

        // Custom field limit
        assert!(validate_multipart_limits(3, 1024, Some(2)).is_err());
        assert!(validate_multipart_limits(2, 1024, Some(2)).is_ok());
    }
}
