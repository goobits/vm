//! Provides file system utility functions for tasks such as project detection and analysis.
//!
//! This module contains helper functions for checking the existence of files and directories,
//! which is a common requirement for detecting the type of a project (e.g., Node.js, Python, etc.).

use std::fs;
use std::path::Path;

/// Checks if a file exists within a given directory.
///
/// # Arguments
///
/// * `dir` - A `Path` to the directory to search in.
/// * `filename` - The name of the file to look for.
///
/// # Returns
///
/// `true` if the file exists, `false` otherwise.
pub fn has_file(dir: &Path, filename: &str) -> bool {
    dir.join(filename).exists()
}

/// Checks if any of the specified files exist in a directory.
///
/// This function is useful for checking for the presence of one of several possible
/// configuration files.
///
/// # Arguments
///
/// * `dir` - A `Path` to the directory to search in.
/// * `filenames` - A slice of file names to look for.
///
/// # Returns
///
/// `true` if at least one of the files exists, `false` otherwise.
pub fn has_any_file(dir: &Path, filenames: &[&str]) -> bool {
    filenames.iter().any(|&filename| has_file(dir, filename))
}

/// Checks if any of the specified directories exist within a directory.
///
/// # Arguments
///
/// * `dir` - A `Path` to the directory to search in.
/// * `dirnames` - A slice of directory names to look for.
///
/// # Returns
///
/// `true` if at least one of the directories exists, `false` otherwise.
pub fn has_any_dir(dir: &Path, dirnames: &[&str]) -> bool {
    dirnames.iter().any(|&dirname| dir.join(dirname).is_dir())
}

/// Checks if a file exists and contains a specific string.
///
/// This function is useful for checking for a specific dependency or configuration
/// setting within a file.
///
/// # Arguments
///
/// * `dir` - A `Path` to the directory where the file is located.
/// * `filename` - The name of the file to check.
/// * `content` - The string to search for within the file.
///
/// # Returns
///
/// `true` if the file exists and contains the specified content, `false` otherwise.
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
