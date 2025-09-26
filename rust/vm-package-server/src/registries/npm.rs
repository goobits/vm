use crate::package_utils::RegistryPattern;
use crate::registry_trait::Registry;
use crate::{AppResult, AppState, SuccessResponse};
use async_trait::async_trait;

/// NPM registry implementation
pub struct NpmRegistry;

#[async_trait]
impl Registry for NpmRegistry {
    fn name(&self) -> &'static str {
        "npm"
    }

    fn pattern(&self) -> &'static RegistryPattern {
        &RegistryPattern::NPM
    }

    fn name_extractor(&self) -> Box<dyn Fn(&str) -> Option<String> + Send + Sync> {
        Box::new(|filename| {
            // Extract package name from JSON metadata filename
            filename.strip_suffix(".json").map(|name| name.to_string())
        })
    }

    async fn delete_package_version(
        &self,
        state: &AppState,
        package_name: &str,
        version: &str,
    ) -> AppResult<SuccessResponse> {
        use tracing::info;

        info!(package = %package_name, version = %version, "Unpublishing NPM package version");

        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{}.json", package_name));
        let tarballs_dir = state.data_dir.join("npm/tarballs");

        // First, update the JSON metadata to remove the version
        crate::deletion::update_json_remove_version(&metadata_path, version)
            .await
            .map_err(|e| {
                let error_msg = e.to_string();
                if error_msg.contains("Package metadata not found") {
                    crate::error::AppError::NotFound(format!(
                        "Package '{}' not found",
                        package_name
                    ))
                } else if error_msg.contains("not found in package metadata") {
                    crate::error::AppError::NotFound(format!(
                        "Version '{}' not found for package '{}'",
                        version, package_name
                    ))
                } else {
                    crate::error::AppError::BadRequest(format!(
                        "Failed to update package metadata: {}",
                        error_msg
                    ))
                }
            })?;

        // Then remove the tarball files for this specific version
        let deleted_files = crate::deletion::remove_files_matching_pattern(
            &tarballs_dir,
            package_name,
            version,
            &[".tgz"],
        )
        .await
        .map_err(|e| {
            crate::error::AppError::BadRequest(format!("Failed to remove tarball files: {}", e))
        })?;

        info!(
            package = %package_name,
            version = %version,
            files = ?deleted_files,
            "Unpublished NPM package version"
        );

        Ok(SuccessResponse {
            message: format!(
                "Unpublished NPM package '{}' version '{}' (updated metadata and deleted {} tarball files)",
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
        use tracing::info;

        info!(package = %package_name, "Deleting all NPM package versions");

        let metadata_path = state
            .data_dir
            .join("npm/metadata")
            .join(format!("{}.json", package_name));
        let tarballs_dir = state.data_dir.join("npm/tarballs");

        // Remove metadata file entirely
        if metadata_path.exists() {
            tokio::fs::remove_file(&metadata_path).await.map_err(|e| {
                crate::error::AppError::BadRequest(format!("Failed to remove metadata file: {}", e))
            })?;
        }

        // Remove all tarball files for this package
        let mut all_deleted_files = Vec::new();
        if tarballs_dir.exists() {
            if let Ok(mut entries) = tokio::fs::read_dir(&tarballs_dir).await {
                while let Some(entry) = entries.next_entry().await.map_err(|e| {
                    crate::error::AppError::BadRequest(format!("Failed to read directory: {}", e))
                })? {
                    let path = entry.path();
                    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                        if filename.starts_with(&format!("{}-", package_name))
                            && filename.ends_with(".tgz")
                        {
                            if let Err(e) = tokio::fs::remove_file(&path).await {
                                tracing::warn!(file = %path.display(), error = %e, "Failed to delete file");
                            } else {
                                all_deleted_files.push(filename.to_string());
                            }
                        }
                    }
                }
            }
        }

        if !metadata_path.exists() && all_deleted_files.is_empty() {
            return Err(crate::error::AppError::NotFound(format!(
                "No files found for package '{}'",
                package_name
            )));
        }

        info!(
            package = %package_name,
            files = ?all_deleted_files,
            "Deleted all NPM package versions"
        );

        Ok(SuccessResponse {
            message: format!(
                "Deleted NPM package '{}' (removed metadata and {} tarball files)",
                package_name,
                all_deleted_files.len()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_npm_test_state;

    #[tokio::test]
    async fn test_npm_registry_count_packages() {
        let (state, _temp_dir) = create_npm_test_state();
        let registry = NpmRegistry;

        // The shared helper should work through the trait
        let count = registry.count_packages(&state).await.unwrap();
        assert_eq!(count, 0); // No packages in fresh test state
    }

    #[tokio::test]
    async fn test_npm_registry_list_packages() {
        let (state, _temp_dir) = create_npm_test_state();
        let registry = NpmRegistry;

        let packages = registry.list_all_packages(&state).await.unwrap();
        assert!(packages.is_empty()); // No packages in fresh test state
    }

    #[tokio::test]
    async fn test_npm_name_extractor() {
        let registry = NpmRegistry;
        let extractor = registry.name_extractor();

        assert_eq!(extractor("express.json"), Some("express".to_string()));
        assert_eq!(
            extractor("@types/node.json"),
            Some("@types/node".to_string())
        );
        assert_eq!(extractor("lodash.json"), Some("lodash".to_string()));
        assert_eq!(extractor("invalid"), None);
        assert_eq!(extractor("file.txt"), None);
    }

    #[tokio::test]
    async fn test_npm_registry_delete_package_version() {
        let (state, _temp_dir) = create_npm_test_state();
        let registry = NpmRegistry;

        // Test deleting a non-existent package (should return error)
        let result = registry
            .delete_package_version(&state, "nonexistent", "1.0.0")
            .await;
        assert!(result.is_err()); // NPM trait returns error for non-existent packages
    }

    #[tokio::test]
    async fn test_npm_registry_delete_all_versions() {
        let (state, _temp_dir) = create_npm_test_state();
        let registry = NpmRegistry;

        // Test deleting a non-existent package (should return error)
        let result = registry.delete_all_versions(&state, "nonexistent").await;
        assert!(result.is_err()); // NPM trait returns error for non-existent packages
    }
}
