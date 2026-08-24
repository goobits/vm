//! Focused filesystem helpers shared by native registry handlers.

use crate::error::AppResult;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// List direct child files matching one of the supplied suffixes.
pub async fn list_files_with_extensions<P: AsRef<Path>>(
    dir: P,
    extensions: &[&str],
) -> AppResult<Vec<PathBuf>> {
    let dir = dir.as_ref();
    let mut files = Vec::new();

    if !dir.exists() {
        debug!(dir = %dir.display(), "Directory does not exist");
        return Ok(files);
    }

    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if extensions.iter().any(|ext| name.ends_with(ext)) {
                files.push(path);
            }
        }
    }

    debug!(
        count = files.len(),
        "Listed files with specified extensions"
    );
    Ok(files)
}

/// Return whether a filename has an allowed suffix.
pub fn validate_file_extension(filename: &str, allowed_extensions: &[&str]) -> bool {
    allowed_extensions.iter().any(|ext| filename.ends_with(ext))
}

/// Read a file size without failing registry listings for missing files.
pub async fn get_file_size<P: AsRef<Path>>(path: P) -> u64 {
    tokio::fs::metadata(path.as_ref())
        .await
        .map(|m| m.len())
        .unwrap_or(0)
}
