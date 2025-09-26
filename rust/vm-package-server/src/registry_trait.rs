use crate::error::AppResult;
use crate::package_utils::RegistryPattern;
use crate::state::AppState;
use async_trait::async_trait;

pub type NameExtractorFn = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Registry trait for abstracting package registry operations
///
/// This trait provides a common interface for different package registries
/// (PyPI, NPM, Cargo) while allowing registry-specific implementations
/// for complex operations like version listings, uploads, and downloads.
///
/// We start with a simple proof-of-concept focusing on basic operations
/// that can be shared, while keeping complex operations in registry modules.
#[async_trait]
pub trait Registry: Send + Sync {
    /// Registry name identifier
    fn name(&self) -> &'static str;

    /// Registry pattern for shared file operations
    fn pattern(&self) -> &'static RegistryPattern;

    /// Count packages using shared pattern helper
    async fn count_packages(&self, state: &AppState) -> AppResult<usize> {
        crate::package_utils::count_packages_by_pattern(
            &state.data_dir,
            self.pattern(),
            self.name_extractor(),
        )
        .await
    }

    /// List all packages using shared pattern helper
    async fn list_all_packages(&self, state: &AppState) -> AppResult<Vec<String>> {
        crate::package_utils::list_packages_by_pattern(
            &state.data_dir,
            self.pattern(),
            self.name_extractor(),
        )
        .await
    }

    /// Registry-specific name extraction logic
    fn name_extractor(&self) -> NameExtractorFn;

    /// Delete a specific package version
    /// Uses shared file removal + registry-specific metadata cleanup
    async fn delete_package_version(
        &self,
        state: &AppState,
        package_name: &str,
        version: &str,
    ) -> AppResult<crate::state::SuccessResponse>;

    /// Delete all versions of a package
    /// Uses shared file removal + registry-specific metadata cleanup
    async fn delete_all_versions(
        &self,
        state: &AppState,
        package_name: &str,
    ) -> AppResult<crate::state::SuccessResponse>;

    // Complex operations remain in registry modules for now
    // Future phases will add: get_package_versions, upload, download
}

/// Registry factory for getting registry instances
pub fn get_registry(name: &str) -> AppResult<Box<dyn Registry>> {
    match name {
        "pypi" => Ok(Box::new(crate::registries::pypi::PyPiRegistry)),
        "npm" => Ok(Box::new(crate::registries::npm::NpmRegistry)),
        "cargo" => Ok(Box::new(crate::registries::cargo::CargoRegistry)),
        _ => Err(crate::error::AppError::BadRequest(format!(
            "Unknown registry: {}",
            name
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_factory() {
        // Test valid registries
        assert!(get_registry("pypi").is_ok());
        assert!(get_registry("npm").is_ok());
        assert!(get_registry("cargo").is_ok());

        // Test invalid registry
        assert!(get_registry("unknown").is_err());
        assert!(get_registry("").is_err());
    }

    #[tokio::test]
    async fn test_registry_trait_methods() {
        let pypi = get_registry("pypi").unwrap();
        assert_eq!(pypi.name(), "pypi");
        assert_eq!(pypi.pattern().subdir, "pypi/packages");

        let npm = get_registry("npm").unwrap();
        assert_eq!(npm.name(), "npm");
        assert_eq!(npm.pattern().subdir, "npm/metadata");

        let cargo = get_registry("cargo").unwrap();
        assert_eq!(cargo.name(), "cargo");
        assert_eq!(cargo.pattern().subdir, "cargo/crates");
    }
}
