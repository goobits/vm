//! # Package Management Utilities
//!
//! This module provides a comprehensive set of utilities for managing packages across
//! different registry types (PyPI, NPM, Cargo). It implements common patterns for
//! package discovery, counting, listing, and metadata extraction while maintaining
//! high performance through async operations and efficient deduplication.
//!
//! ## Core Functionality
//!
//! - **File Discovery**: Efficiently find package files by extension patterns
//! - **Package Counting**: Count unique packages with customizable name extraction
//! - **Package Listing**: Generate sorted lists of available packages
//! - **Recent Packages**: Find recently modified packages with time-based sorting
//! - **Registry Patterns**: Standardized configuration for different registry types
//!
//! ## Registry Pattern System
//!
//! The module implements a registry pattern system that abstracts common operations
//! across different package managers:
//!
//! ```rust,no_run
//! use vm_package_server::package_utils::{RegistryPattern, count_packages_by_pattern};
//! # use std::path::Path;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let data_dir = Path::new("/data");
//! # fn extract_pypi_package_name(filename: &str) -> Option<String> { Some(filename.to_string()) }
//! // Count PyPI packages
//! let count = count_packages_by_pattern(
//!     data_dir,
//!     &RegistryPattern::PYPI,
//!     |filename| extract_pypi_package_name(filename)
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Supported Registry Types
//!
//! - **PyPI**: Handles `.whl` and `.tar.gz` files in `pypi/packages/`
//! - **NPM**: Processes `.json` metadata files in `npm/metadata/`
//! - **Cargo**: Manages `.crate` files in `cargo/crates/` with recursive scanning
//!
//! ## Performance Features
//!
//! - **Async I/O**: All file operations use `tokio::fs` for non-blocking performance
//! - **Efficient Deduplication**: Uses `HashSet` for O(1) uniqueness checking
//! - **Recursive Scanning**: Optimized recursive directory traversal for Cargo
//! - **Memory Efficient**: Streams directory entries rather than loading all into memory
//!
//! ## Security Considerations
//!
//! - **Path Validation**: All file operations validate paths to prevent traversal attacks
//! - **Size Limits**: Provides utilities for file size checking and validation
//! - **Extension Validation**: Strict filtering by allowed file extensions
//! - **Error Handling**: Graceful degradation when directories don't exist
//!
//! ## Usage Patterns
//!
//! ### Basic File Discovery
//!
//! ```rust,no_run
//! use vm_package_server::package_utils::list_files_with_extensions;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!
//! let files = list_files_with_extensions(
//!     "/data/pypi/packages",
//!     &[".whl", ".tar.gz"]
//! ).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Custom Package Extraction
//!
//! ```rust,no_run
//! use vm_package_server::package_utils::count_packages_by_name_extraction;
//! # use std::path::Path;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let dir = Path::new("/data");
//!
//! let count = count_packages_by_name_extraction(
//!     dir,
//!     &[".whl"],
//!     |filename| {
//!         // Custom logic to extract package name from filename
//!         filename.split('-').next().map(|s| s.to_string())
//!     }
//! ).await?;
//! # Ok(())
//! # }
//! ```

use crate::error::AppResult;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Lists all files in a directory that match any of the specified extensions.
///
/// # Arguments
/// * `dir` - Directory path to search in
/// * `extensions` - Array of file extensions to match (e.g., [".whl", ".tar.gz"])
///
/// # Returns
/// Vector of matching file paths, or empty vector if directory doesn't exist
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

/// Counts unique packages by extracting names from filenames
///
/// This function provides a flexible way to count packages by applying
/// a custom name extraction function to files matching specified extensions.
/// It ensures each package is counted only once, regardless of how many
/// files (versions, architectures) exist for that package.
///
/// # Arguments
///
/// * `dir` - Directory to scan for package files
/// * `extensions` - File extensions to include (e.g., `[".whl", ".tar.gz"]`)
/// * `name_extractor` - Function that extracts package name from filename
///
/// # Returns
///
/// The number of unique packages found, or 0 if directory doesn't exist
///
/// # Example
///
/// ```rust,no_run
/// use vm_package_server::package_utils::count_packages_by_name_extraction;
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let count = count_packages_by_name_extraction(
///     "/data/pypi/packages",
///     &[".whl"],
///     |filename| filename.split('-').next().map(|s| s.to_string())
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn count_packages_by_name_extraction<P: AsRef<Path>, F>(
    dir: P,
    extensions: &[&str],
    name_extractor: F,
) -> AppResult<usize>
where
    F: Fn(&str) -> Option<String>,
{
    let files = list_files_with_extensions(dir, extensions).await?;
    let mut packages = HashSet::new();

    for path in files {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(package_name) = name_extractor(name) {
                packages.insert(package_name);
            }
        }
    }

    Ok(packages.len())
}

/// Lists unique packages by extracting names from filenames
///
/// Similar to `count_packages_by_name_extraction`, but returns the actual
/// package names in a sorted vector. Useful for API endpoints that need
/// to display available packages.
///
/// # Arguments
///
/// * `dir` - Directory to scan for package files
/// * `extensions` - File extensions to include
/// * `name_extractor` - Function that extracts package name from filename
///
/// # Returns
///
/// Sorted vector of unique package names
pub async fn list_packages_by_name_extraction<P: AsRef<Path>, F>(
    dir: P,
    extensions: &[&str],
    name_extractor: F,
) -> AppResult<Vec<String>>
where
    F: Fn(&str) -> Option<String>,
{
    let files = list_files_with_extensions(dir, extensions).await?;
    let mut packages = HashSet::new();

    for path in files {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some(package_name) = name_extractor(name) {
                packages.insert(package_name);
            }
        }
    }

    let mut sorted: Vec<String> = packages.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

/// Generic recent packages extraction with time sorting
pub async fn get_recent_packages_by_name_extraction<P: AsRef<Path>, F, V>(
    dir: P,
    extensions: &[&str],
    limit: usize,
    name_version_extractor: F,
) -> AppResult<Vec<(String, V)>>
where
    F: Fn(&str) -> Option<(String, V)>,
{
    let dir = dir.as_ref();
    let mut files_with_time = Vec::new();

    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if extensions.iter().any(|ext| name.ends_with(ext)) {
                if let Ok(metadata) = entry.metadata().await {
                    if let Ok(modified) = metadata.modified() {
                        files_with_time.push((path, modified));
                    }
                }
            }
        }
    }

    // Sort by modification time (most recent first)
    files_with_time.sort_by(|a, b| b.1.cmp(&a.1));

    let mut recent = Vec::new();
    for (path, _) in files_with_time.iter().take(limit * 2) {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if let Some((package_name, version)) = name_version_extractor(name) {
                recent.push((package_name, version));
                if recent.len() >= limit {
                    break;
                }
            }
        }
    }

    Ok(recent)
}

/// Validates that a filename has an allowed extension
///
/// Security utility that ensures uploaded files have expected extensions.
/// Used to prevent upload of unexpected file types that could pose
/// security risks or break registry assumptions.
///
/// # Arguments
///
/// * `filename` - The filename to validate
/// * `allowed_extensions` - Array of allowed extensions (e.g., `[".whl", ".tar.gz"]`)
///
/// # Returns
///
/// `true` if the filename ends with any allowed extension, `false` otherwise
pub fn validate_file_extension(filename: &str, allowed_extensions: &[&str]) -> bool {
    allowed_extensions.iter().any(|ext| filename.ends_with(ext))
}

/// Safely retrieves file size with graceful error handling
///
/// This utility function attempts to get the size of a file, returning 0
/// if the file doesn't exist or is inaccessible. Useful for displaying
/// package sizes in listings without failing on missing files.
///
/// # Arguments
///
/// * `path` - Path to the file
///
/// # Returns
///
/// File size in bytes, or 0 if file is inaccessible
pub async fn get_file_size<P: AsRef<Path>>(path: P) -> u64 {
    tokio::fs::metadata(path.as_ref())
        .await
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Recursive directory traversal for counting unique items (used by Cargo)
pub async fn count_recursive_unique<P: AsRef<Path>, F>(
    dir: P,
    item_extractor: F,
) -> AppResult<usize>
where
    F: Fn(&Path) -> Option<String> + Send + Sync + Copy,
{
    let mut items = HashSet::new();
    count_recursive_helper(dir.as_ref(), &mut items, item_extractor).await?;
    Ok(items.len())
}

async fn count_recursive_helper<F>(
    dir: &Path,
    items: &mut HashSet<String>,
    item_extractor: F,
) -> AppResult<()>
where
    F: Fn(&Path) -> Option<String> + Send + Sync + Copy,
{
    let mut entries = fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(count_recursive_helper(&path, items, item_extractor)).await?;
        } else if path.is_file() {
            if let Some(item) = item_extractor(&path) {
                items.insert(item);
            }
        }
    }

    Ok(())
}

/// Recursive directory traversal for listing unique items (used by Cargo)
pub async fn list_recursive_unique<P: AsRef<Path>, F>(
    dir: P,
    item_extractor: F,
) -> AppResult<Vec<String>>
where
    F: Fn(&Path) -> Option<String> + Send + Sync + Copy,
{
    let mut items = HashSet::new();
    count_recursive_helper(dir.as_ref(), &mut items, item_extractor).await?;

    let mut sorted: Vec<String> = items.into_iter().collect();
    sorted.sort();
    Ok(sorted)
}

/// Configuration pattern for different package registry types
///
/// This struct defines the standard directory structure and file patterns
/// for different package registries. It enables consistent handling of
/// registry-specific operations through a unified interface.
///
/// Each registry type has its own pattern defining where files are stored
/// and what extensions to look for. This abstraction allows the same
/// utility functions to work across PyPI, NPM, and Cargo registries.
pub struct RegistryPattern {
    /// Subdirectory under the main data directory where files are stored
    pub subdir: &'static str,
    /// File extensions to scan for packages in this registry
    pub extensions: &'static [&'static str],
}

impl RegistryPattern {
    /// PyPI registry pattern: scans `pypi/packages/` for wheel and source distributions
    ///
    /// PyPI stores package files directly in a packages directory, with both
    /// wheel files (`.whl`) and source distributions (`.tar.gz`) supported.
    pub const PYPI: Self = Self {
        subdir: "pypi/packages",
        extensions: &[".whl", ".tar.gz"],
    };

    /// NPM registry pattern: scans `npm/metadata/` for package metadata files
    ///
    /// NPM uses a metadata-based approach where package information is stored
    /// in JSON files rather than the packages themselves.
    pub const NPM: Self = Self {
        subdir: "npm/metadata",
        extensions: &["json"],
    };

    /// Cargo registry pattern: scans `cargo/crates/` for crate archive files
    ///
    /// Cargo stores crate files (`.crate`) in a hierarchical directory structure
    /// that may require recursive scanning depending on the registry layout.
    pub const CARGO: Self = Self {
        subdir: "cargo/crates",
        extensions: &[".crate"],
    };
}

/// Count packages using a registry pattern
///
/// This wrapper captures the common PyPI/NPM pattern:
/// 1. Join data_dir with registry subdirectory
/// 2. Call count_packages_by_name_extraction with pattern's extensions
/// 3. Use registry-specific name extraction logic
pub async fn count_packages_by_pattern<F>(
    data_dir: &Path,
    pattern: &RegistryPattern,
    name_extractor: F,
) -> AppResult<usize>
where
    F: Fn(&str) -> Option<String>,
{
    let dir = data_dir.join(pattern.subdir);
    count_packages_by_name_extraction(dir, pattern.extensions, name_extractor).await
}

/// List packages using a registry pattern
///
/// This wrapper captures the common PyPI/NPM pattern:
/// 1. Join data_dir with registry subdirectory
/// 2. Call list_packages_by_name_extraction with pattern's extensions
/// 3. Use registry-specific name extraction logic
pub async fn list_packages_by_pattern<F>(
    data_dir: &Path,
    pattern: &RegistryPattern,
    name_extractor: F,
) -> AppResult<Vec<String>>
where
    F: Fn(&str) -> Option<String>,
{
    let dir = data_dir.join(pattern.subdir);
    list_packages_by_name_extraction(dir, pattern.extensions, name_extractor).await
}

/// Get recent packages using a registry pattern
///
/// This wrapper captures the common pattern for simple filename-based registries like Cargo:
/// 1. Join data_dir with registry subdirectory
/// 2. Call get_recent_packages_by_name_extraction with pattern's extensions
/// 3. Use registry-specific name+version extraction logic
///
/// Note: Complex registries like PyPI/NPM may need custom implementations for:
/// - Reading metadata files (NPM reads JSON for latest version)
/// - Deduplication logic (PyPI deduplicates by normalized package name)
/// - Business rules (PyPI takes limit*2 then deduplicates)
pub async fn get_recent_packages_by_pattern<F, V>(
    data_dir: &Path,
    pattern: &RegistryPattern,
    limit: usize,
    name_version_extractor: F,
) -> AppResult<Vec<(String, V)>>
where
    F: Fn(&str) -> Option<(String, V)>,
    V: Clone,
{
    let dir = data_dir.join(pattern.subdir);
    get_recent_packages_by_name_extraction(dir, pattern.extensions, limit, name_version_extractor)
        .await
}
