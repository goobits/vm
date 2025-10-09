//! # Input Validation: Path and Filename Validation
//!
//! This module provides helpers for validating file paths and names to prevent
//! path traversal attacks and ensure filesystem safety.

use crate::validation::error::ValidationError;
use crate::validation::limits::MAX_PATH_DEPTH;
use crate::validation::result::ValidationResult;
use std::path::{Path, PathBuf};

/// Validate that a path is safe from directory traversal attacks.
///
/// This function checks for path traversal attempts, ensures the path is relative,
/// and validates that it doesn't exceed maximum depth limits. It also checks for
/// dangerous characters and patterns.
///
/// # Arguments
///
/// * `path` - The path to validate
///
/// # Returns
///
/// `Ok(PathBuf)` if the path is safe, `Err(ValidationError)` otherwise
///
/// # Examples
///
/// ```rust
/// use vm_package_server::validation::paths::validate_safe_path;
///
/// // Safe relative path
/// // assert!(validate_safe_path("packages/mypackage/1.0.0").is_ok());
///
/// // Path traversal attempt
/// // assert!(validate_safe_path("../../../etc/passwd").is_err());
///
/// // Absolute path
/// // assert!(validate_safe_path("/etc/passwd").is_err());
/// ```
pub fn validate_safe_path<P: AsRef<Path>>(path: P) -> ValidationResult<PathBuf> {
    let path = path.as_ref();
    let path_str = path.to_string_lossy();

    // Check for null bytes
    if path_str.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    // Check for control characters
    if path_str
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
    {
        return Err(ValidationError::ControlCharacters);
    }

    // Reject absolute paths
    if path.is_absolute() {
        return Err(ValidationError::AbsolutePath {
            path: path_str.to_string(),
        });
    }

    // Check for path traversal patterns
    if path_str.contains("..") {
        return Err(ValidationError::PathTraversal {
            path: path_str.to_string(),
        });
    }

    // Check path depth
    let depth = path.components().count();
    if depth > MAX_PATH_DEPTH {
        return Err(ValidationError::PathTooDeep {
            actual: depth,
            max: MAX_PATH_DEPTH,
        });
    }

    // Additional checks for dangerous patterns
    let dangerous_patterns = [
        "//", "\\\\", "~", "$", "`", "|", "&", ";", "<", ">", "(", ")", "{", "}", "[", "]", "*",
        "?",
    ];

    for pattern in &dangerous_patterns {
        if path_str.contains(pattern) {
            return Err(ValidationError::InvalidCharacters {
                input: path_str.to_string(),
            });
        }
    }

    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_safe_path() {
        // Valid paths
        assert!(validate_safe_path("packages/mypackage").is_ok());
        assert!(validate_safe_path("data/npm/tarballs/package.tgz").is_ok());

        // Invalid paths
        assert!(validate_safe_path("../../../etc/passwd").is_err());
        assert!(validate_safe_path("/etc/passwd").is_err());
        assert!(validate_safe_path("path/with/../traversal").is_err());
        assert!(validate_safe_path("path/with/null\0byte").is_err());
    }
}