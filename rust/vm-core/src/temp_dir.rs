//! Provides utilities for creating and managing temporary files and directories.
//!
//! This module simplifies the creation of temporary files and directories by providing
//! convenient wrappers around the `tempfile` crate. The resources are automatically
//! cleaned up when they go out of scope.

use std::io::Result;
use tempfile::{Builder, NamedTempFile, TempDir};

/// Creates a secure temporary file with a specified prefix and suffix.
///
/// The file is automatically deleted when the returned `NamedTempFile` object is dropped.
///
/// # Arguments
///
/// * `prefix` - A string slice to use as the prefix for the temporary file's name.
/// * `suffix` - A string slice to use as the suffix for the temporary file's name.
///
/// # Returns
///
/// A `Result` containing the `NamedTempFile` on success, or an `std::io::Error` on failure.
pub fn create_temp_file(prefix: &str, suffix: &str) -> Result<NamedTempFile> {
    Builder::new().prefix(prefix).suffix(suffix).tempfile()
}

/// Creates a secure temporary directory with a specified prefix.
///
/// The directory and its contents are automatically deleted when the returned `TempDir`
/// object is dropped.
///
/// # Arguments
///
/// * `prefix` - A string slice to use as the prefix for the temporary directory's name.
///
/// # Returns
///
/// A `Result` containing the `TempDir` on success, or an `std::io::Error` on failure.
pub fn create_temp_dir(prefix: &str) -> Result<TempDir> {
    Builder::new().prefix(prefix).tempdir()
}
