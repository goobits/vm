//! # Input Validation: Path and Filename Validation
//!
//! This module provides helpers for validating file paths and names to prevent
//! path traversal attacks and ensure filesystem safety.

use crate::validation::error::ValidationError;
use crate::validation::limits::{MAX_FILENAME_LENGTH, MAX_PATH_DEPTH};
use crate::validation::result::ValidationResult;
use crate::{AppError, AppResult};
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

/// Validate a client-provided leaf filename before joining it to managed storage.
pub fn validate_filename(filename: &str) -> AppResult<()> {
    if filename.is_empty() {
        return Err(AppError::BadRequest("Filename cannot be empty".into()));
    }
    if filename.len() > MAX_FILENAME_LENGTH {
        return Err(AppError::BadRequest(format!(
            "Filename too long: {} characters (max: {MAX_FILENAME_LENGTH})",
            filename.len()
        )));
    }
    if filename.chars().any(char::is_control) {
        return Err(AppError::BadRequest(
            "Filename contains control characters".into(),
        ));
    }
    if filename.contains("..") {
        return Err(AppError::BadRequest(
            "Filename contains parent directory reference (..)".into(),
        ));
    }
    if filename.contains(['/', '\\']) {
        return Err(AppError::BadRequest(
            "Filename cannot contain path separators".into(),
        ));
    }
    if filename.as_bytes().get(1) == Some(&b':') && filename.as_bytes()[0].is_ascii_alphabetic() {
        return Err(AppError::BadRequest(
            "Filename cannot contain drive letter".into(),
        ));
    }

    const RESERVED_WINDOWS_NAMES: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let base_name = filename.rsplit_once('.').map_or(filename, |(base, _)| base);
    if RESERVED_WINDOWS_NAMES
        .iter()
        .any(|reserved| base_name.eq_ignore_ascii_case(reserved))
    {
        return Err(AppError::BadRequest(format!(
            "Filename '{base_name}' is reserved on Windows systems"
        )));
    }
    Ok(())
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

    #[test]
    fn filename_rejects_path_segments_and_reserved_names() {
        assert!(validate_filename("safe_file.txt").is_ok());
        assert!(validate_filename("nested/file.txt").is_err());
        assert!(validate_filename(r"nested\file.txt").is_err());
        assert!(validate_filename("CON.txt").is_err());
    }
}
