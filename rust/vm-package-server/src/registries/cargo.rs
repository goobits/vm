use crate::package_utils::RegistryPattern;
use crate::registry_trait::Registry;
use crate::{AppResult, AppState, SuccessResponse};
use async_trait::async_trait;
use tracing::info;

/// Cargo registry implementation
pub struct CargoRegistry;

#[async_trait]
impl Registry for CargoRegistry {
    fn name(&self) -> &'static str {
        "cargo"
    }

    fn pattern(&self) -> &'static RegistryPattern {
        &RegistryPattern::CARGO
    }

    fn name_extractor(&self) -> Box<dyn Fn(&str) -> Option<String> + Send + Sync> {
        Box::new(|filename| {
            // Extract crate name from .crate filename: name-version.crate
            if let Some(name_version) = filename.strip_suffix(".crate") {
                // Find the last dash that separates name from version
                if let Some(dash_pos) = name_version.rfind('-') {
                    Some(name_version[..dash_pos].to_string())
                } else {
                    // Fallback: assume entire string is the name
                    Some(name_version.to_string())
                }
            } else {
                None
            }
        })
    }

    /// Override count_packages to use recursive counting since Cargo has a different directory structure
    async fn count_packages(&self, state: &AppState) -> AppResult<usize> {
        let crates_dir = state.data_dir.join("cargo/crates");
        crate::package_utils::count_recursive_unique(crates_dir, |path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|filename| (self.name_extractor())(filename))
        })
        .await
    }

    /// Override list_all_packages to use recursive listing since Cargo has a different directory structure
    async fn list_all_packages(&self, state: &AppState) -> AppResult<Vec<String>> {
        let crates_dir = state.data_dir.join("cargo/crates");
        crate::package_utils::list_recursive_unique(crates_dir, |path| {
            path.file_name()
                .and_then(|n| n.to_str())
                .and_then(|filename| (self.name_extractor())(filename))
        })
        .await
    }

    async fn delete_package_version(
        &self,
        _state: &AppState,
        package_name: &str,
        version: &str,
    ) -> AppResult<SuccessResponse> {
        // Simplified implementation for trait demonstration
        // Complex Cargo deletion (with index management) remains in cargo.rs module
        info!(crate_name = %package_name, version = %version, "Cargo trait delete not fully implemented");

        Err(crate::error::AppError::BadRequest(
            "Cargo deletion via trait not fully implemented. Use module-specific handlers."
                .to_string(),
        ))
    }

    async fn delete_all_versions(
        &self,
        _state: &AppState,
        package_name: &str,
    ) -> AppResult<SuccessResponse> {
        // Simplified implementation for trait demonstration
        // Complex Cargo deletion (with index management) remains in cargo.rs module
        info!(crate_name = %package_name, "Cargo trait delete not fully implemented");

        Err(crate::error::AppError::BadRequest(
            "Cargo deletion via trait not fully implemented. Use module-specific handlers."
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_cargo_test_state;

    #[tokio::test]
    async fn test_cargo_registry_count_packages() {
        let (state, _temp_dir) = create_cargo_test_state();
        let registry = CargoRegistry;

        // The shared helper should work through the trait
        let count = registry.count_packages(&state).await.unwrap();
        assert_eq!(count, 0); // No packages in fresh test state
    }

    #[tokio::test]
    async fn test_cargo_registry_list_packages() {
        let (state, _temp_dir) = create_cargo_test_state();
        let registry = CargoRegistry;

        let packages = registry.list_all_packages(&state).await.unwrap();
        assert!(packages.is_empty()); // No packages in fresh test state
    }

    #[tokio::test]
    async fn test_cargo_name_extractor() {
        let registry = CargoRegistry;
        let extractor = registry.name_extractor();

        assert_eq!(extractor("serde-1.0.0.crate"), Some("serde".to_string()));
        assert_eq!(extractor("tokio-1.28.0.crate"), Some("tokio".to_string()));
        assert_eq!(
            extractor("my-crate-0.1.0.crate"),
            Some("my-crate".to_string())
        );
        assert_eq!(
            extractor("name-with-many-dashes-1.0.0.crate"),
            Some("name-with-many-dashes".to_string())
        );
        assert_eq!(extractor("invalid.txt"), None);
        assert_eq!(extractor("nocrate"), None);

        // Edge case: crate with no version-like suffix
        assert_eq!(extractor("justname.crate"), Some("justname".to_string()));
    }

    #[tokio::test]
    async fn test_cargo_registry_delete_not_implemented() {
        let (state, _temp_dir) = create_cargo_test_state();
        let registry = CargoRegistry;

        // Test that delete operations return not implemented errors
        let result = registry
            .delete_package_version(&state, "serde", "1.0.0")
            .await;
        assert!(result.is_err());

        let result = registry.delete_all_versions(&state, "serde").await;
        assert!(result.is_err());
    }
}
