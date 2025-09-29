use std::io::Result;
use tempfile::{Builder, NamedTempFile, TempDir};

/// Creates a secure temporary file.
/// The file is automatically deleted when the `NamedTempFile` object is dropped.
pub fn create_temp_file(prefix: &str, suffix: &str) -> Result<NamedTempFile> {
    Builder::new().prefix(prefix).suffix(suffix).tempfile()
}

/// Creates a secure temporary directory.
/// The directory is automatically deleted when the `TempDir` object is dropped.
pub fn create_temp_dir(prefix: &str) -> Result<TempDir> {
    Builder::new().prefix(prefix).tempdir()
}
