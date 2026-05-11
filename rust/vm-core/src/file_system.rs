//! File system utility functions for project detection and analysis.

use std::fs;
use std::io;
use std::path::Path;

/// Write `content` to `path` atomically.
///
/// The data is first written to a sibling temporary file in the same directory,
/// then renamed into place. A crash mid-write therefore leaves the destination
/// untouched instead of truncating it to a half-written state. The temp file is
/// removed if the rename step fails so we don't leak `.tmp` files into user
/// state directories.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("atomic_write: path has no file name: {}", path.display()),
        )
    })?;

    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = path.with_file_name(tmp_name);

    fs::write(&tmp_path, content)?;
    if let Err(e) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e);
    }
    Ok(())
}

/// Check if a file exists in a directory
pub fn has_file(dir: &Path, filename: &str) -> bool {
    dir.join(filename).exists()
}

/// Check if any of the specified files exist in a directory
pub fn has_any_file(dir: &Path, filenames: &[&str]) -> bool {
    filenames.iter().any(|&filename| has_file(dir, filename))
}

/// Check if any of the specified directories exist in a directory
pub fn has_any_dir(dir: &Path, dirnames: &[&str]) -> bool {
    dirnames.iter().any(|&dirname| dir.join(dirname).is_dir())
}

/// Check if a file exists and contains a specific string
pub fn has_file_containing(dir: &Path, filename: &str, content: &str) -> bool {
    let file_path = dir.join(filename);
    if !file_path.exists() {
        return false;
    }

    if let Ok(file_contents) = fs::read_to_string(file_path) {
        file_contents.contains(content)
    } else {
        false
    }
}
