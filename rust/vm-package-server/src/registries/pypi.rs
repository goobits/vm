use crate::normalize_pypi_name;
use crate::package_utils::RegistryPattern;
use crate::registry_trait::Registry;
use crate::{AppResult, AppState, SuccessResponse};
use async_trait::async_trait;
use tracing::info;

/// PyPI registry implementation
pub struct PyPiRegistry;

#[async_trait]
impl Registry for PyPiRegistry {
    fn name(&self) -> &'static str {
        "pypi"
    }

    fn pattern(&self) -> &'static RegistryPattern {
        &RegistryPattern::PYPI
    }

    fn name_extractor(&self) -> Box<dyn Fn(&str) -> Option<String> + Send + Sync> {
        Box::new(|filename| filename.split('-').next().map(normalize_pypi_name))
    }

    async fn delete_package_version(
        &self,
        state: &AppState,
        package_name: &str,
        version: &str,
    ) -> AppResult<SuccessResponse> {
        info!(package = %package_name, version = %version, "Deleting PyPI package version");

        let pypi_dir = state.data_dir.join("pypi/packages");
        let normalized_name = normalize_pypi_name(package_name);

        // Use the deletion helper to remove files and .meta files
        let deleted_files = crate::deletion::remove_files_matching_pattern(
            &pypi_dir,
            &normalized_name,
            version,
            &[".whl", ".tar.gz"],
        )
        .await
        .map_err(crate::error::AppError::Anyhow)?;

        if deleted_files.is_empty() {
            return Err(crate::error::AppError::NotFound(format!(
                "No files found for package '{}' version '{}'",
                package_name, version
            )));
        }

        info!(
            package = %package_name,
            version = %version,
            files = ?deleted_files,
            "Deleted PyPI package version"
        );

        Ok(SuccessResponse {
            message: format!(
                "Deleted PyPI package '{}' version '{}' ({} files)",
                package_name,
                version,
                deleted_files.len()
            ),
        })
    }

    async fn delete_all_versions(
        &self,
        state: &AppState,
        package_name: &str,
    ) -> AppResult<SuccessResponse> {
        info!(package = %package_name, "Deleting all PyPI package versions");

        let pypi_dir = state.data_dir.join("pypi/packages");
        let normalized_name = normalize_pypi_name(package_name);

        // Find all files for this package
        let mut all_deleted_files = Vec::new();

        if let Ok(mut entries) = tokio::fs::read_dir(&pypi_dir).await {
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(pkg_name) = filename.split('-').next() {
                        if normalize_pypi_name(pkg_name) == normalized_name {
                            if let Err(e) = tokio::fs::remove_file(&path).await {
                                tracing::warn!(file = %path.display(), error = %e, "Failed to delete file");
                            } else {
                                all_deleted_files.push(filename.to_string());

                                // Also remove corresponding .meta file if it exists
                                let meta_path = path.with_extension(format!(
                                    "{}.meta",
                                    path.extension().unwrap_or_default().to_str().unwrap_or("")
                                ));
                                if meta_path.exists() {
                                    if let Err(e) = tokio::fs::remove_file(&meta_path).await {
                                        tracing::warn!(file = %meta_path.display(), error = %e, "Failed to delete meta file");
                                    } else {
                                        let meta_filename = meta_path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("unknown.meta");
                                        all_deleted_files.push(meta_filename.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if all_deleted_files.is_empty() {
            return Err(crate::error::AppError::NotFound(format!(
                "No files found for package '{}'",
                package_name
            )));
        }

        info!(
            package = %package_name,
            files = ?all_deleted_files,
            "Deleted all PyPI package versions"
        );

        Ok(SuccessResponse {
            message: format!(
                "Deleted {} files for package '{}'",
                all_deleted_files.len(),
                package_name
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_pypi_test_state;

    #[tokio::test]
    async fn test_pypi_registry_count_packages() {
        let (state, _temp_dir) = create_pypi_test_state();
        let registry = PyPiRegistry;

        // The shared helper should work through the trait
        let count = registry.count_packages(&state).await.unwrap();
        assert_eq!(count, 0); // No packages in fresh test state
    }

    #[tokio::test]
    async fn test_pypi_registry_list_packages() {
        let (state, _temp_dir) = create_pypi_test_state();
        let registry = PyPiRegistry;

        let packages = registry.list_all_packages(&state).await.unwrap();
        assert!(packages.is_empty()); // No packages in fresh test state
    }

    #[tokio::test]
    async fn test_pypi_registry_delete_package_version() {
        let (state, _temp_dir) = create_pypi_test_state();
        let registry = PyPiRegistry;

        // Test deleting a non-existent package
        let result = registry
            .delete_package_version(&state, "nonexistent", "1.0.0")
            .await;
        assert!(result.is_err()); // Should return NotFound error
    }

    #[tokio::test]
    async fn test_pypi_registry_delete_all_versions() {
        let (state, _temp_dir) = create_pypi_test_state();
        let registry = PyPiRegistry;

        // Test deleting a non-existent package
        let result = registry.delete_all_versions(&state, "nonexistent").await;
        assert!(result.is_err()); // Should return NotFound error
    }
}
