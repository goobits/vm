//! # Package Registry Trait System
//!
//! This module defines the core trait-based architecture for package registries.
//! It provides a unified interface for different package management systems (NPM, PyPI, Cargo)
//! while allowing registry-specific implementations.
//!
//! ## Design Goals
//!
//! - **Abstraction**: Unified interface for common package operations
//! - **Extensibility**: Easy to add new registry types
//! - **Type Safety**: Strong typing for package metadata and operations
//! - **Async**: All operations are async for non-blocking I/O
//!
//! ## Architecture
//!
//! The trait system separates common operations from registry-specific logic:
//!
//! ```text
//! PackageRegistry (trait)
//!     ├── NpmRegistry (impl)
//!     ├── PypiRegistry (impl)
//!     └── CargoRegistry (impl, future)
//! ```
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use vm_package_server::registry::{PackageRegistry, NpmRegistry};
//! # use vm_package_server::AppState;
//! # async fn example(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
//! let registry = NpmRegistry::new();
//! let count = registry.count_packages(state).await?;
//! println!("NPM packages: {}", count);
//! # Ok(())
//! # }
//! ```

use crate::{AppError, AppResult, AppState};
use serde::{Deserialize, Serialize};

/// Metadata for package uploads across all registries.
///
/// This struct captures common metadata fields needed when uploading packages
/// to any registry type. Registry-specific implementations can extract additional
/// information from their specific formats (e.g., package.json for NPM, setup.py for PyPI).
///
/// # Fields
///
/// * `name` - Package name (normalized per registry conventions)
/// * `version` - Package version string (must follow registry's version format)
/// * `description` - Optional package description
/// * `author` - Optional author information
/// * `license` - Optional license identifier
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    /// Package name (should be normalized according to registry rules)
    pub name: String,
    /// Version string (format depends on registry)
    pub version: String,
    /// Optional package description
    pub description: Option<String>,
    /// Optional author information
    pub author: Option<String>,
    /// Optional license identifier (e.g., "MIT", "Apache-2.0")
    pub license: Option<String>,
}

/// Core trait for package registry operations.
///
/// This trait defines the common interface that all package registries must implement.
/// It provides async methods for the fundamental operations: counting, listing, versioning,
/// downloading, and uploading packages.
///
/// # Implementation Notes
///
/// - All methods are async to support non-blocking I/O operations
/// - Methods take `&self` to support stateless implementations (zero-sized types)
/// - Registry-specific HTML endpoints (like PyPI's simple index) are NOT in the trait
/// - Error handling uses the shared `AppResult` type
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to support multi-threaded web servers.
///
/// # Example Implementation
///
/// ```rust,ignore
/// struct MyRegistry;
///
/// #[async_trait::async_trait]
/// impl PackageRegistry for MyRegistry {
///     async fn count_packages(&self, state: &AppState) -> AppResult<usize> {
///         // Implementation...
///     }
///     // ... other methods
/// }
/// ```
#[async_trait::async_trait]
pub trait PackageRegistry: Send + Sync {
    /// Count the total number of unique packages in the registry.
    ///
    /// This should count unique packages, not individual versions or files.
    /// For example, if a package has 5 versions, it should be counted as 1 package.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state containing the data directory and configuration
    ///
    /// # Returns
    ///
    /// The number of unique packages, or an error if the operation fails
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let count = registry.count_packages(&state).await?;
    /// println!("Total packages: {}", count);
    /// ```
    async fn count_packages(&self, state: &AppState) -> AppResult<usize>;

    /// List all package names in the registry (sorted alphabetically).
    ///
    /// Returns a sorted list of all unique package names. The names should be
    /// normalized according to the registry's conventions (e.g., lowercase for PyPI).
    ///
    /// # Arguments
    ///
    /// * `state` - Application state containing the data directory and configuration
    ///
    /// # Returns
    ///
    /// Vector of package names sorted alphabetically, or an error
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let packages = registry.list_all_packages(&state).await?;
    /// for package in packages {
    ///     println!("- {}", package);
    /// }
    /// ```
    async fn list_all_packages(&self, state: &AppState) -> AppResult<Vec<String>>;

    /// Get all versions for a specific package.
    ///
    /// Returns version information including download URLs, hashes, and file sizes.
    /// The exact format of the tuple elements depends on the registry type.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state containing the data directory and configuration
    /// * `package_name` - The package name to query (should match registry's naming convention)
    ///
    /// # Returns
    ///
    /// Vector of tuples containing:
    /// - `String`: Version identifier
    /// - `String`: Download URL or filename
    /// - `String`: Hash (SHA1 for NPM, SHA256 for PyPI)
    /// - `u64`: File size in bytes
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let versions = registry.get_package_versions(&state, "express").await?;
    /// for (version, url, hash, size) in versions {
    ///     println!("{}: {} bytes, hash={}", version, size, hash);
    /// }
    /// ```
    async fn get_package_versions(
        &self,
        state: &AppState,
        package_name: &str,
    ) -> AppResult<Vec<(String, String, String, u64)>>;

    /// Get recently updated packages.
    ///
    /// Returns the most recently modified packages, useful for displaying
    /// "what's new" or activity feeds.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state containing the data directory and configuration
    /// * `limit` - Maximum number of packages to return
    ///
    /// # Returns
    ///
    /// Vector of tuples containing:
    /// - `String`: Package name
    /// - `String`: Latest version
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let recent = registry.get_recent_packages(&state, 10).await?;
    /// for (name, version) in recent {
    ///     println!("Recently updated: {} ({})", name, version);
    /// }
    /// ```
    async fn get_recent_packages(
        &self,
        state: &AppState,
        limit: usize,
    ) -> AppResult<Vec<(String, String)>>;

    /// Download a package file.
    ///
    /// Returns the raw binary data for a specific package version.
    /// The implementation should handle local storage lookup and optionally
    /// fall back to upstream registries if the file is not found locally.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state containing the data directory and configuration
    /// * `package_name` - The package name
    /// * `version` - The version to download (or filename for some registries)
    ///
    /// # Returns
    ///
    /// Binary data of the package file, or an error if not found
    ///
    /// # Security
    ///
    /// Implementations must validate inputs to prevent path traversal attacks.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let data = registry.download_package(&state, "lodash", "4.17.21").await?;
    /// println!("Downloaded {} bytes", data.len());
    /// ```
    async fn download_package(
        &self,
        state: &AppState,
        package_name: &str,
        version: &str,
    ) -> AppResult<Vec<u8>>;

    /// Upload/publish a package to the registry.
    ///
    /// Stores a new package or package version in the registry. The implementation
    /// should validate the package data, calculate hashes, and store both the
    /// package file and metadata.
    ///
    /// # Arguments
    ///
    /// * `state` - Application state containing the data directory and configuration
    /// * `package_data` - Raw binary data of the package file
    /// * `metadata` - Package metadata (name, version, etc.)
    ///
    /// # Returns
    ///
    /// Success or error
    ///
    /// # Security
    ///
    /// Implementations must:
    /// - Validate file sizes
    /// - Check filename safety
    /// - Verify package integrity
    /// - Sanitize metadata
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let metadata = PackageMetadata {
    ///     name: "my-package".to_string(),
    ///     version: "1.0.0".to_string(),
    ///     description: Some("A great package".to_string()),
    ///     author: None,
    ///     license: Some("MIT".to_string()),
    /// };
    /// registry.upload_package(&state, package_data, metadata).await?;
    /// ```
    async fn upload_package(
        &self,
        state: &AppState,
        package_data: Vec<u8>,
        metadata: PackageMetadata,
    ) -> AppResult<()>;

    /// Get the registry name identifier.
    ///
    /// Returns a string identifier for the registry type (e.g., "npm", "pypi", "cargo").
    /// This is useful for logging, error messages, and UI display.
    ///
    /// # Returns
    ///
    /// Static string slice identifying the registry
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// println!("Using {} registry", registry.registry_name());
    /// ```
    fn registry_name(&self) -> &str;
}

// Submodules
pub mod npm_registry;
pub mod pypi_registry;

// Re-export implementations
pub use npm_registry::NpmRegistry;
pub use pypi_registry::PypiRegistry;

/// Get a registry implementation by name.
///
/// This factory function allows runtime selection of registry implementations
/// based on a string identifier. Useful for dynamic dispatch or configuration-driven
/// registry selection.
///
/// # Arguments
///
/// * `name` - Registry identifier ("npm", "pypi", or "cargo")
///
/// # Returns
///
/// Boxed trait object for the requested registry, or an error if unknown
///
/// # Example
///
/// ```rust
/// use vm_package_server::registry::get_registry;
///
/// let npm = get_registry("npm").unwrap();
/// assert_eq!(npm.registry_name(), "npm");
///
/// let pypi = get_registry("pypi").unwrap();
/// assert_eq!(pypi.registry_name(), "pypi");
/// ```
pub fn get_registry(name: &str) -> Result<Box<dyn PackageRegistry>, AppError> {
    match name.to_lowercase().as_str() {
        "npm" => Ok(Box::new(NpmRegistry::new())),
        "pypi" => Ok(Box::new(PypiRegistry::new())),
        _ => Err(AppError::NotFound(format!(
            "Unknown registry type: {}. Supported: npm, pypi",
            name
        ))),
    }
}

/// Constant reference to NPM registry instance.
///
/// Since `NpmRegistry` is a zero-sized type, this is a convenient constant
/// for accessing the NPM registry without creating new instances.
///
/// # Example
///
/// ```rust
/// use vm_package_server::registry::{NPM, PackageRegistry};
/// # use vm_package_server::AppState;
/// # async fn example(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
/// let count = NPM.count_packages(state).await?;
/// println!("NPM packages: {}", count);
/// # Ok(())
/// # }
/// ```
pub const NPM: NpmRegistry = NpmRegistry;

/// Constant reference to PyPI registry instance.
///
/// Since `PypiRegistry` is a zero-sized type, this is a convenient constant
/// for accessing the PyPI registry without creating new instances.
///
/// # Example
///
/// ```rust
/// use vm_package_server::registry::{PYPI, PackageRegistry};
/// # use vm_package_server::AppState;
/// # async fn example(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
/// let count = PYPI.count_packages(state).await?;
/// println!("PyPI packages: {}", count);
/// # Ok(())
/// # }
/// ```
pub const PYPI: PypiRegistry = PypiRegistry;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_registry_npm() {
        let registry = get_registry("npm").unwrap();
        assert_eq!(registry.registry_name(), "npm");
    }

    #[test]
    fn test_get_registry_pypi() {
        let registry = get_registry("pypi").unwrap();
        assert_eq!(registry.registry_name(), "pypi");
    }

    #[test]
    fn test_get_registry_case_insensitive() {
        assert!(get_registry("NPM").is_ok());
        assert!(get_registry("PyPI").is_ok());
        assert!(get_registry("PYPI").is_ok());
    }

    #[test]
    fn test_get_registry_unknown() {
        let result = get_registry("cargo");
        assert!(result.is_err());
        if let Err(AppError::NotFound(msg)) = result {
            assert!(msg.contains("Unknown registry type"));
            assert!(msg.contains("cargo"));
        } else {
            panic!("Expected NotFound error");
        }
    }

    #[test]
    fn test_constants() {
        assert_eq!(NPM.registry_name(), "npm");
        assert_eq!(PYPI.registry_name(), "pypi");
    }
}
