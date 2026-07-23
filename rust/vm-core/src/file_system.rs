//! File system utility functions for project detection and analysis.

use std::fs;
use std::io;
use std::path::Path;

/// Replace a file without exposing partially written contents.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path has no file name: {}", path.display()),
        )
    })?;
    let mut temporary_name = file_name.to_os_string();
    temporary_name.push(".tmp");
    let temporary_path = path.with_file_name(temporary_name);

    fs::write(&temporary_path, content)?;
    if let Err(error) = fs::rename(&temporary_path, path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(error);
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
