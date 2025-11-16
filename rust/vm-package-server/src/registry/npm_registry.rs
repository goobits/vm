//! # NPM Registry Implementation
//!
//! This module provides the NPM-specific implementation of the `PackageRegistry` trait.
//! It handles NPM package operations including metadata management, tarball storage,
//! and version tracking.
//!
//! ## NPM Package Structure
//!
//! NPM packages are stored with the following structure:
//! - Metadata: `npm/metadata/{package}.json` - Contains all versions and metadata
//! - Tarballs: `npm/tarballs/{package}-{version}.tgz` - Actual package files
//!
//! ## Key Features
//!
//! - JSON-based metadata storage (follows NPM registry format)
//! - SHA1 hash verification for package integrity
//! - Support for scoped packages (@scope/package)
//! - Tarball URL generation for downloads
//!
//! ## Example
//!
//! ```rust,no_run
//! use vm_package_server::registry::{PackageRegistry, NpmRegistry};
//! # use vm_package_server::AppState;
//! # async fn example(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
//! let npm = NpmRegistry::new();
//! let packages = npm.list_all_packages(state).await?;
//! println!("NPM packages: {:?}", packages);
//! # Ok(())
//! # }
//! ```

use crate::registry::{PackageMetadata, PackageRegistry};
use crate::{package_utils, storage, AppResult, AppState};
use serde_json::Value;
use tracing::debug;

/// NPM package registry implementation.
///
/// This is a zero-sized type that implements the `PackageRegistry` trait
/// for NPM packages. All state is stored in the `AppState` data directory.
///
/// # Storage Layout
///
/// - `{data_dir}/npm/metadata/` - Package metadata JSON files
/// - `{data_dir}/npm/tarballs/` - Package tarball (.tgz) files
#[derive(Clone, Copy)]
pub struct NpmRegistry;

impl NpmRegistry {
    /// Create a new NPM registry instance.
    ///
    /// Since this is a zero-sized type, this is essentially a no-op,
    /// but provides a consistent interface with other registry types.
    ///
    /// # Example
    ///
    /// ```rust
    /// use vm_package_server::registry::NpmRegistry;
    /// let npm = NpmRegistry::new();
    /// ```
    pub fn new() -> Self {
        Self
    }
}

impl Default for NpmRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PackageRegistry for NpmRegistry {
    async fn count_packages(&self, state: &AppState) -> AppResult<usize> {
        package_utils::count_packages_by_pattern(
            &state.data_dir,
            &package_utils::RegistryPattern::NPM,
            |filename| {
                // Extract package name from filename (remove .json extension)
                filename.strip_suffix(".json").map(|name| name.to_string())
            },
        )
        .await
    }

    async fn list_all_packages(&self, state: &AppState) -> AppResult<Vec<String>> {
        package_utils::list_packages_by_pattern(
            &state.data_dir,
            &package_utils::RegistryPattern::NPM,
            |filename| {
                // Extract package name from filename (remove .json extension)
                filename.strip_suffix(".json").map(|name| name.to_string())
            },
        )
        .await
    }

    async fn get_package_versions(
        &self,
        state: &AppState,
        package_name: &str,
    ) -> AppResult<Vec<(String, String, String, u64)>> {
        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{package_name}.json"));
        let mut versions = Vec::new();

        let result = storage::read_file_string(&metadata_path).await;
        if let Ok(content) = result {
            if let Ok(metadata) = serde_json::from_str::<Value>(&content) {
                if let Some(versions_obj) = metadata["versions"].as_object() {
                    for (version, data) in versions_obj {
                        if let Some(dist) = data["dist"].as_object() {
                            let tarball = dist
                                .get("tarball")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let shasum = dist
                                .get("shasum")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();

                            // Extract filename from tarball URL and get file size
                            let filename = tarball.split('/').next_back().unwrap_or("");
                            let file_path = state.data_dir.join("npm/tarballs").join(filename);
                            let size = package_utils::get_file_size(&file_path).await;

                            versions.push((version.clone(), tarball, shasum, size));
                        }
                    }
                }
            }
        }

        versions.sort_by(|a, b| b.0.cmp(&a.0)); // Sort versions in reverse
        Ok(versions)
    }

    async fn get_recent_packages(
        &self,
        state: &AppState,
        limit: usize,
    ) -> AppResult<Vec<(String, String)>> {
        // Get recent metadata files using pattern helper, then read JSON to extract latest versions
        let recent_files = package_utils::get_recent_packages_by_pattern(
            &state.data_dir,
            &package_utils::RegistryPattern::NPM,
            limit,
            |filename| {
                // Extract package name from filename (remove .json extension)
                filename
                    .strip_suffix(".json")
                    .map(|name| (name.to_string(), String::new()))
            },
        )
        .await?;

        let mut recent = Vec::new();
        for (package_name, _) in recent_files {
            // Read metadata file to get latest version
            let metadata_path = state
                .data_dir
                .join("npm/metadata")
                .join(format!("{package_name}.json"));
            if let Ok(content) = storage::read_file_string(&metadata_path).await {
                if let Ok(metadata) = serde_json::from_str::<Value>(&content) {
                    if let Some(latest) = metadata["dist-tags"]["latest"].as_str() {
                        recent.push((package_name, latest.to_string()));
                    }
                }
            }
        }

        Ok(recent)
    }

    async fn download_package(
        &self,
        state: &AppState,
        package_name: &str,
        version: &str,
    ) -> AppResult<Vec<u8>> {
        // For NPM, version is actually the filename (e.g., "express-4.18.2.tgz")
        // This matches the existing download_tarball handler signature
        debug!(package = %package_name, filename = %version, "Downloading npm tarball via registry trait");

        let file_path = state.data_dir.join("npm/tarballs").join(version);

        // Try local file first
        match storage::read_file(&file_path).await {
            Ok(data) => {
                debug!(package = %package_name, filename = %version, size = data.len(), "Serving tarball from local storage");
                Ok(data)
            }
            Err(_) => {
                // File not found locally, try upstream NPM
                debug!(package = %package_name, filename = %version, "Tarball not found locally, checking upstream NPM");

                // Construct the tarball URL for upstream
                let tarball_url = format!("/{package_name}/-/{version}");
                match state.upstream_client.stream_npm_tarball(&tarball_url).await {
                    Ok(bytes) => {
                        debug!(package = %package_name, filename = %version, size = bytes.len(), "Streaming tarball from upstream NPM");
                        Ok(bytes.to_vec())
                    }
                    Err(e) => {
                        debug!(package = %package_name, filename = %version, error = %e, "Tarball not found on upstream NPM either");
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
        // Note: NPM upload is handled via the existing publish_package handler
        // which uses a different format (JSON with base64-encoded attachments).
        // This trait method is a simplified interface that would need adapter logic.
        // For Phase 1 & 2, we're keeping existing upload handlers unchanged.

        // This is a placeholder that would be implemented in Phase 3 when
        // integrating the trait into the actual handlers.
        debug!(registry = %self.registry_name(), "Upload via trait not yet implemented (use existing handlers)");
        Err(crate::AppError::NotImplemented(
            "NPM upload via trait interface not yet implemented. Use existing npm::publish_package handler.".to_string()
        ))
    }

    fn registry_name(&self) -> &str {
        "npm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_npm_test_state;
    use serde_json::json;

    #[tokio::test]
    async fn test_count_packages() {
        let (state, _temp_dir) = create_npm_test_state();

        // Create test metadata files
        let metadata1 = json!({"name": "package1", "versions": {}});
        let metadata2 = json!({"name": "package2", "versions": {}});

        let metadata_dir = state.data_dir.join("npm/metadata");
        std::fs::write(
            metadata_dir.join("package1.json"),
            serde_json::to_string(&metadata1).unwrap(),
        )
        .unwrap();
        std::fs::write(
            metadata_dir.join("package2.json"),
            serde_json::to_string(&metadata2).unwrap(),
        )
        .unwrap();

        let registry = NpmRegistry::new();
        let count = registry.count_packages(&state).await.unwrap();
        assert_eq!(count, 2, "Should count 2 packages");
    }

    #[tokio::test]
    async fn test_list_all_packages() {
        let (state, _temp_dir) = create_npm_test_state();

        // Create test metadata files
        let metadata_dir = state.data_dir.join("npm/metadata");
        std::fs::write(metadata_dir.join("express.json"), "{}").unwrap();
        std::fs::write(metadata_dir.join("lodash.json"), "{}").unwrap();
        std::fs::write(metadata_dir.join("axios.json"), "{}").unwrap();

        let registry = NpmRegistry::new();
        let packages = registry.list_all_packages(&state).await.unwrap();

        assert_eq!(packages.len(), 3);
        assert_eq!(packages, vec!["axios", "express", "lodash"]); // Should be sorted
    }

    #[tokio::test]
    async fn test_get_package_versions() {
        let (state, _temp_dir) = create_npm_test_state();

        let metadata = json!({
            "name": "test-pkg",
            "versions": {
                "1.0.0": {
                    "dist": {
                        "tarball": "http://localhost:8080/npm/test-pkg/-/test-pkg-1.0.0.tgz",
                        "shasum": "abc123"
                    }
                },
                "2.0.0": {
                    "dist": {
                        "tarball": "http://localhost:8080/npm/test-pkg/-/test-pkg-2.0.0.tgz",
                        "shasum": "def456"
                    }
                }
            }
        });

        let metadata_path = state.data_dir.join("npm/metadata/test-pkg.json");
        std::fs::write(
            &metadata_path,
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .unwrap();

        let registry = NpmRegistry::new();
        let versions = registry
            .get_package_versions(&state, "test-pkg")
            .await
            .unwrap();

        assert_eq!(versions.len(), 2);
        // Should be sorted in reverse order (2.0.0 first)
        assert_eq!(versions[0].0, "2.0.0");
        assert_eq!(versions[1].0, "1.0.0");
        assert_eq!(versions[0].2, "def456"); // hash
        assert_eq!(versions[1].2, "abc123");
    }

    #[tokio::test]
    async fn test_registry_name() {
        let registry = NpmRegistry::new();
        assert_eq!(registry.registry_name(), "npm");
    }
}
