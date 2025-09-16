//! File system detection and validation utilities.
//!
//! This module provides functions for detecting files, directories, and file content
//! patterns used throughout the VM tool for environment detection and validation.
//! These utilities are the foundation for framework detection and configuration
//! validation across different project types.

use std::fs;
use std::path::Path;

/// Checks if a file exists in the base directory.
pub fn has_file(base_dir: &Path, file_name: &str) -> bool {
    base_dir.join(file_name).exists()
}

/// Checks if any of the specified files exist in the base directory.
pub fn has_any_file(base_dir: &Path, file_names: &[&str]) -> bool {
    file_names.iter().any(|f| has_file(base_dir, f))
}

/// Checks if any of the specified directories exist in the base directory.
pub fn has_any_dir(base_dir: &Path, dir_names: &[&str]) -> bool {
    dir_names.iter().any(|d| base_dir.join(d).is_dir())
}

/// Checks if a file contains a specific string pattern.
///
/// This function reads the entire file content into memory and searches for the pattern.
/// For large files, consider using streaming approaches if memory usage is a concern.
pub fn has_file_containing(base_dir: &Path, file_name: &str, pattern: &str) -> bool {
    match fs::read_to_string(base_dir.join(file_name)) {
        Ok(content) => content.contains(pattern),
        Err(_) => false,
    }
}
