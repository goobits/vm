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
pub fn has_file_containing(base_dir: &Path, file_name: &str, pattern: &str) -> bool {
    if let Ok(content) = fs::read_to_string(base_dir.join(file_name)) {
        return content.contains(pattern);
    }
    false
}