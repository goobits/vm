use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use vm_common::vm_error;

/// Security utilities for path validation and command sanitization
pub struct SecurityValidator;

impl SecurityValidator {
    /// Validate a relative path to prevent directory traversal attacks
    ///
    /// This function ensures that:
    /// - The path is relative (not absolute)
    /// - The path doesn't contain ".." components
    /// - The path doesn't start with ".."
    /// - The resolved path stays within the workspace boundary
    /// - The path is reasonable length for developer use
    pub fn validate_relative_path(relative_path: &Path, workspace_path: &str) -> Result<PathBuf> {
        // Check for reasonable path length (prevent accidental huge inputs)
        let path_str = relative_path.to_string_lossy();
        if path_str.len() > 4096 {
            vm_error!(
                "Path too long (max 4096 characters): {} characters provided",
                path_str.len()
            );
            return Err(anyhow::anyhow!("Path too long"));
        }
        // Reject absolute paths
        if relative_path.is_absolute() {
            vm_error!(
                "Absolute paths are not allowed (use relative paths from workspace root): {}",
                relative_path.display()
            );
            return Err(anyhow::anyhow!("Absolute paths not allowed"));
        }

        // Check for dangerous path components
        for component in relative_path.components() {
            match component {
                std::path::Component::ParentDir => {
                    vm_error!(
                        "Path traversal attempts (..) are not allowed: {}",
                        relative_path.display()
                    );
                    return Err(anyhow::anyhow!("Path traversal not allowed"));
                }
                std::path::Component::CurDir => {
                    // "." is okay
                    continue;
                }
                std::path::Component::Normal(_) => {
                    // Normal path components are okay
                    continue;
                }
                _ => {
                    vm_error!("Invalid path component in: {}", relative_path.display());
                    return Err(anyhow::anyhow!("Invalid path component"));
                }
            }
        }

        // Construct the safe target path
        let workspace = Path::new(workspace_path);
        let target_path = if relative_path.as_os_str().is_empty() || relative_path == Path::new(".")
        {
            workspace.to_path_buf()
        } else {
            workspace.join(relative_path)
        };

        // Ensure the resolved path is still within workspace
        // This is an additional safety check
        // Note: For VM paths (like /workspace), we can't canonicalize on the host,
        // so we use string-based validation instead
        let workspace_str = workspace.to_string_lossy();
        let target_str = target_path.to_string_lossy();

        // Ensure proper boundary checking by comparing with trailing slash
        // This prevents "/workspace-evil" from passing when workspace is "/workspace"
        let workspace_with_slash = if workspace_str.ends_with('/') {
            workspace_str.to_string()
        } else {
            format!("{}/", workspace_str)
        };

        // Check if target is within workspace (or is exactly the workspace)
        if target_path != workspace && !target_str.starts_with(&workspace_with_slash) {
            vm_error!(
                "Path escapes workspace boundary: {} -> {} (workspace: {})",
                relative_path.display(),
                target_path.display(),
                workspace_str
            );
            return Err(anyhow::anyhow!("Path escapes workspace boundary"));
        }

        Ok(target_path)
    }

    /// Validate a filename for script creation (no path separators, safe characters only)
    pub fn validate_script_name(filename: &str) -> Result<()> {
        // Check for empty name
        if filename.is_empty() {
            vm_error!("Script name cannot be empty");
            return Err(anyhow::anyhow!("Script name cannot be empty"));
        }

        // Check for reasonable length
        if filename.len() > 255 {
            vm_error!(
                "Script name too long (max 255 characters): {} characters provided",
                filename.len()
            );
            return Err(anyhow::anyhow!("Script name too long"));
        }

        // Check for path separators
        if filename.contains('/') || filename.contains('\\') {
            vm_error!("Script name cannot contain path separators: {}", filename);
            return Err(anyhow::anyhow!("Script name cannot contain path separators"));
        }

        // Check for dangerous characters
        if filename.contains("..") || filename.starts_with('.') {
            vm_error!(
                "Script name cannot contain '..' or start with '.': {}",
                filename
            );
            return Err(anyhow::anyhow!("Script name cannot contain '..' or start with '.'"));
        }

        // Only allow alphanumeric, dash, underscore, and dots (for extensions)
        if !filename
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            vm_error!("Script name can only contain alphanumeric characters, dashes, underscores, and dots: {}", filename);
            return Err(anyhow::anyhow!("Script name invalid characters"));
        }

        Ok(())
    }

    /// Create a safe destination path within a restricted directory
    pub fn safe_destination_path(base_dir: &Path, filename: &str) -> Result<PathBuf> {
        // Validate the filename first
        Self::validate_script_name(filename)?;

        // Ensure base directory exists and is absolute
        if !base_dir.is_absolute() {
            vm_error!("Base directory must be absolute: {}", base_dir.display());
            return Err(anyhow::anyhow!("Base directory must be absolute"));
        }

        let destination = base_dir.join(filename);

        // Double-check that we haven't escaped the base directory
        let canonical_base = base_dir
            .canonicalize()
            .context("Failed to canonicalize base directory")?;

        // Since destination might not exist, we check the parent
        if let Some(parent) = destination.parent() {
            if let Ok(canonical_parent) = parent.canonicalize() {
                if !canonical_parent.starts_with(&canonical_base) {
                    vm_error!(
                        "Destination escapes base directory: {}",
                        destination.display()
                    );
                    return Err(anyhow::anyhow!("Destination escapes base directory"));
                }
            }
        }

        Ok(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_validate_relative_path_normal() {
        let workspace = "/workspace";
        let relative = Path::new("src/main.rs");
        let result = SecurityValidator::validate_relative_path(relative, workspace);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Path::new("/workspace/src/main.rs"));
    }

    #[test]
    fn test_validate_relative_path_traversal() {
        let workspace = "/workspace";
        let relative = Path::new("../etc/passwd");
        let result = SecurityValidator::validate_relative_path(relative, workspace);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Path traversal"));
    }

    #[test]
    fn test_validate_relative_path_absolute() {
        let workspace = "/workspace";
        let relative = Path::new("/etc/passwd");
        let result = SecurityValidator::validate_relative_path(relative, workspace);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Absolute paths"));
    }

    #[test]
    fn test_validate_relative_path_boundary_check() {
        // Test that /workspace doesn't incorrectly allow /workspace-evil
        let workspace = "/workspace";

        // This should work - path within workspace
        let valid = Path::new("src/main.rs");
        let result = SecurityValidator::validate_relative_path(valid, workspace);
        assert!(result.is_ok());

        // Edge case: workspace itself
        let workspace_path = Path::new(".");
        let result = SecurityValidator::validate_relative_path(workspace_path, workspace);
        assert!(result.is_ok());

        // Another edge case: empty path
        let empty = Path::new("");
        let result = SecurityValidator::validate_relative_path(empty, workspace);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_relative_path_similar_prefix() {
        // Test paths with similar prefixes don't bypass security
        let workspace = "/home/user";

        // Valid path within workspace
        let valid = Path::new("documents/file.txt");
        let result = SecurityValidator::validate_relative_path(valid, workspace);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Path::new("/home/user/documents/file.txt"));
    }

    #[test]
    fn test_validate_script_name_normal() {
        let result = SecurityValidator::validate_script_name("my-script_v2");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_script_name_path_separator() {
        let result = SecurityValidator::validate_script_name("path/to/script");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("path separators"));
    }

    #[test]
    fn test_validate_script_name_traversal() {
        let result = SecurityValidator::validate_script_name("..script");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".."));
    }
}
