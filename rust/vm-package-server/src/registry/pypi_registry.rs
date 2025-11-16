//! # PyPI Registry Implementation
//!
//! This module provides the PyPI-specific implementation of the `PackageRegistry` trait.
//! It handles Python package operations including wheel and source distribution storage,
//! package indexing, and PEP 503 compatibility.
//!
//! ## PyPI Package Structure
//!
//! PyPI packages are stored with the following structure:
//! - Packages: `pypi/packages/{package}-{version}-{platform}.whl` - Wheel files
//! - Packages: `pypi/packages/{package}-{version}.tar.gz` - Source distributions
//! - Metadata: `pypi/packages/{filename}.{ext}.meta` - SHA256 hash metadata
//!
//! ## Key Features
//!
//! - PEP 503 simple repository API compliance
//! - SHA256 hash verification for package integrity
//! - Support for both wheel (.whl) and source (.tar.gz) distributions
//! - Package name normalization per PEP 503 rules
//!
//! ## Example
//!
//! ```rust,no_run
//! use vm_package_server::registry::{PackageRegistry, PypiRegistry};
//! # use vm_package_server::AppState;
//! # async fn example(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
//! let pypi = PypiRegistry::new();
//! let packages = pypi.list_all_packages(state).await?;
//! println!("PyPI packages: {:?}", packages);
//! # Ok(())
//! # }
//! ```

use crate::registry::{PackageMetadata, PackageRegistry};
use crate::{normalize_pypi_name, package_utils, storage, AppResult, AppState};
use std::collections::HashSet;
use std::path::Path;
use tracing::debug;

/// Helper to list valid package files (.whl, .tar.gz) in the PyPI packages directory
async fn list_package_files(pypi_dir: &Path) -> AppResult<Vec<std::path::PathBuf>> {
    package_utils::list_files_with_extensions(pypi_dir, &[".whl", ".tar.gz"]).await
}

/// PyPI package registry implementation.
///
/// This is a zero-sized type that implements the `PackageRegistry` trait
/// for PyPI packages. All state is stored in the `AppState` data directory.
///
/// # Storage Layout
///
/// - `{data_dir}/pypi/packages/` - Package files (.whl and .tar.gz)
/// - `{data_dir}/pypi/packages/{file}.{ext}.meta` - SHA256 hash metadata
///
/// # Package Name Normalization
///
/// PyPI package names are normalized according to PEP 503:
/// - Convert to lowercase
/// - Replace runs of `[-_.]+` with a single dash `-`
#[derive(Clone, Copy)]
pub struct PypiRegistry;

impl PypiRegistry {
    /// Create a new PyPI registry instance.
    ///
    /// Since this is a zero-sized type, this is essentially a no-op,
    /// but provides a consistent interface with other registry types.
    ///
    /// # Example
    ///
    /// ```rust
    /// use vm_package_server::registry::PypiRegistry;
    /// let pypi = PypiRegistry::new();
    /// ```
    pub fn new() -> Self {
        Self
    }
}

impl Default for PypiRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PackageRegistry for PypiRegistry {
    async fn count_packages(&self, state: &AppState) -> AppResult<usize> {
        package_utils::count_packages_by_pattern(
            &state.data_dir,
            &package_utils::RegistryPattern::PYPI,
            crate::utils::extract_pypi_package_name,
        )
        .await
    }

    async fn list_all_packages(&self, state: &AppState) -> AppResult<Vec<String>> {
        package_utils::list_packages_by_pattern(
            &state.data_dir,
            &package_utils::RegistryPattern::PYPI,
            crate::utils::extract_pypi_package_name,
        )
        .await
    }

    async fn get_package_versions(
        &self,
        state: &AppState,
        package_name: &str,
    ) -> AppResult<Vec<(String, String, String, u64)>> {
        let normalized_package = normalize_pypi_name(package_name);
        let pypi_dir = state.data_dir.join("pypi/packages");

        // For PyPI, we return a flattened list of (version, filename, hash, size)
        // Note: PyPI can have multiple files per version (different platforms/pythons)
        let mut results = Vec::new();

        for path in list_package_files(&pypi_dir).await? {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(pkg_name) = crate::utils::extract_pypi_package_name(name) {
                    if pkg_name == normalized_package {
                        // Extract version from filename
                        let version = if let Some((_, ver)) =
                            crate::utils::extract_pypi_package_name_and_version(name)
                        {
                            ver
                        } else {
                            "unknown".to_string()
                        };

                        let meta_path = path.with_extension(format!(
                            "{}.meta",
                            path.extension().and_then(|ext| ext.to_str()).unwrap_or("")
                        ));

                        let hash = storage::read_file_string(&meta_path)
                            .await
                            .unwrap_or_else(|_| "unknown".to_string());

                        // Get file size
                        let size = package_utils::get_file_size(&path).await;

                        // Return (version, filename, hash, size)
                        results.push((version, name.to_string(), hash.trim().to_string(), size));
                    }
                }
            }
        }

        // Sort by version in reverse order
        results.sort_by(|a, b| b.0.cmp(&a.0));
        Ok(results)
    }

    async fn get_recent_packages(
        &self,
        state: &AppState,
        limit: usize,
    ) -> AppResult<Vec<(String, String)>> {
        // Get raw recent files using pattern helper, then apply PyPI deduplication
        let recent_files = package_utils::get_recent_packages_by_pattern(
            &state.data_dir,
            &package_utils::RegistryPattern::PYPI,
            limit, // Helper already handles limit * 2 internally for deduplication
            crate::utils::extract_pypi_package_name_and_version,
        )
        .await?;

        // Deduplicate by package name (keep first occurrence which is most recent)
        let mut seen_packages = HashSet::new();
        let mut recent = Vec::new();

        for (package_name, version) in recent_files {
            if !seen_packages.contains(&package_name) {
                seen_packages.insert(package_name.clone());
                recent.push((package_name, version));
                if recent.len() >= limit {
                    break;
                }
            }
        }

        Ok(recent)
    }

    async fn download_package(
        &self,
        state: &AppState,
        _package_name: &str,
        version: &str,
    ) -> AppResult<Vec<u8>> {
        // For PyPI, version is actually the filename (e.g., "package-1.0.0-py3-none-any.whl")
        // This matches the existing download_file handler signature
        debug!(filename = %version, "Downloading PyPI package file via registry trait");

        let file_path = state.data_dir.join("pypi/packages").join(version);

        // Try local file first
        match storage::read_file(&file_path).await {
            Ok(data) => {
                debug!(filename = %version, size = data.len(), "Serving file from local storage");
                Ok(data)
            }
            Err(_) => {
                // File not found locally, try upstream PyPI
                debug!(filename = %version, "File not found locally, checking upstream PyPI");
                match state.upstream_client.stream_pypi_file(version).await {
                    Ok(bytes) => {
                        debug!(filename = %version, size = bytes.len(), "Streaming file from upstream PyPI");
                        Ok(bytes.to_vec())
                    }
                    Err(e) => {
                        debug!(filename = %version, error = %e, "File not found on upstream PyPI either");
                        Err(e)
                    }
                }
            }
        }
    }

    async fn upload_package(
        &self,
        _state: &AppState,
        _package_data: Vec<u8>,
        _metadata: PackageMetadata,
    ) -> AppResult<()> {
        // Note: PyPI upload is handled via the existing upload_package handler
        // which uses multipart form data with specific field names.
        // This trait method is a simplified interface that would need adapter logic.
        // For Phase 1 & 2, we're keeping existing upload handlers unchanged.

        // This is a placeholder that would be implemented in Phase 3 when
        // integrating the trait into the actual handlers.
        debug!(registry = %self.registry_name(), "Upload via trait not yet implemented (use existing handlers)");
        Err(crate::AppError::NotImplemented(
            "PyPI upload via trait interface not yet implemented. Use existing pypi::upload_package handler.".to_string()
        ))
    }

    fn registry_name(&self) -> &str {
        "pypi"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256_hash;
    use crate::test_utils::create_pypi_test_state;

    #[tokio::test]
    async fn test_count_packages() {
        let (state, _temp_dir) = create_pypi_test_state();

        // Create test package files
        let packages_dir = state.data_dir.join("pypi/packages");
        std::fs::write(packages_dir.join("package1-1.0.0.whl"), b"fake content").unwrap();
        std::fs::write(packages_dir.join("package1-2.0.0.whl"), b"fake content").unwrap();
        std::fs::write(packages_dir.join("package2-1.0.0.tar.gz"), b"fake content").unwrap();

        let registry = PypiRegistry::new();
        let count = registry.count_packages(&state).await.unwrap();
        assert_eq!(
            count, 2,
            "Should count 2 unique packages (package1 and package2)"
        );
    }

    #[tokio::test]
    async fn test_list_all_packages() {
        let (state, _temp_dir) = create_pypi_test_state();

        // Create test package files
        let packages_dir = state.data_dir.join("pypi/packages");
        std::fs::write(packages_dir.join("django-3.2.0.whl"), b"fake").unwrap();
        std::fs::write(packages_dir.join("flask-2.0.0.tar.gz"), b"fake").unwrap();
        std::fs::write(packages_dir.join("requests-2.28.0.whl"), b"fake").unwrap();

        let registry = PypiRegistry::new();
        let packages = registry.list_all_packages(&state).await.unwrap();

        assert_eq!(packages.len(), 3);
        assert_eq!(packages, vec!["django", "flask", "requests"]); // Should be sorted
    }

    #[tokio::test]
    async fn test_list_packages_with_normalization() {
        let (state, _temp_dir) = create_pypi_test_state();

        // Create test package files with names that need normalization
        let packages_dir = state.data_dir.join("pypi/packages");
        std::fs::write(packages_dir.join("My_Package-1.0.0.whl"), b"fake").unwrap();
        std::fs::write(packages_dir.join("my.package-2.0.0.whl"), b"fake").unwrap();
        std::fs::write(packages_dir.join("my-package-3.0.0.tar.gz"), b"fake").unwrap();

        let registry = PypiRegistry::new();
        let packages = registry.list_all_packages(&state).await.unwrap();

        // All three should normalize to "my-package" and be counted as one package
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0], "my-package");
    }

    #[tokio::test]
    async fn test_get_package_versions() {
        let (state, _temp_dir) = create_pypi_test_state();

        let packages_dir = state.data_dir.join("pypi/packages");

        // Create test package files
        let file1 = b"content for version 1";
        let file2 = b"content for version 2";

        std::fs::write(packages_dir.join("testpkg-1.0.0-py3-none-any.whl"), file1).unwrap();
        std::fs::write(packages_dir.join("testpkg-2.0.0-py3-none-any.whl"), file2).unwrap();

        // Create corresponding meta files
        let hash1 = sha256_hash(file1);
        let hash2 = sha256_hash(file2);
        std::fs::write(
            packages_dir.join("testpkg-1.0.0-py3-none-any.whl.meta"),
            &hash1,
        )
        .unwrap();
        std::fs::write(
            packages_dir.join("testpkg-2.0.0-py3-none-any.whl.meta"),
            &hash2,
        )
        .unwrap();

        let registry = PypiRegistry::new();
        let versions = registry
            .get_package_versions(&state, "testpkg")
            .await
            .unwrap();

        assert_eq!(versions.len(), 2);
        // Should be sorted in reverse order (2.0.0 first)
        assert_eq!(versions[0].0, "2.0.0");
        assert_eq!(versions[1].0, "1.0.0");
        assert_eq!(versions[0].2.trim(), hash2.trim()); // hash
        assert_eq!(versions[1].2.trim(), hash1.trim());
    }

    #[tokio::test]
    async fn test_registry_name() {
        let registry = PypiRegistry::new();
        assert_eq!(registry.registry_name(), "pypi");
    }

    #[tokio::test]
    async fn test_get_recent_packages() {
        let (state, _temp_dir) = create_pypi_test_state();

        let packages_dir = state.data_dir.join("pypi/packages");

        // Create test packages with slight time delays
        std::fs::write(packages_dir.join("old-package-1.0.0.whl"), b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        std::fs::write(packages_dir.join("newer-package-1.0.0.whl"), b"newer").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));

        std::fs::write(packages_dir.join("newest-package-1.0.0.whl"), b"newest").unwrap();

        let registry = PypiRegistry::new();
        let recent = registry.get_recent_packages(&state, 2).await.unwrap();

        assert_eq!(recent.len(), 2);
        // Most recent should be first
        assert_eq!(recent[0].0, "newest-package");
        assert_eq!(recent[1].0, "newer-package");
    }
}
