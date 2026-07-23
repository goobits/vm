//! File system utility functions for project detection and analysis.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Replace a file without exposing partially written contents.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    if path.file_name().is_none() || path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path is not a writable file target: {}", path.display()),
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".vm-write-")
        .tempfile_in(parent)?;
    temporary.write_all(content)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map(|_| ())
        .map_err(|error| error.error)
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

#[cfg(test)]
mod tests {
    use super::atomic_write;

    #[test]
    fn atomic_write_replaces_existing_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("generated.yml");
        std::fs::write(&path, "old").unwrap();

        atomic_write(&path, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "new");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    #[test]
    fn atomic_write_rejects_directory_paths() {
        let directory = tempfile::tempdir().unwrap();
        let error = atomic_write(directory.path(), b"content").unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}
