use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

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
            anyhow::bail!("Path too long (max 4096 characters): {} characters provided", path_str.len());
        }
        // Reject absolute paths
        if relative_path.is_absolute() {
            anyhow::bail!("Absolute paths are not allowed (use relative paths from workspace root): {}", relative_path.display());
        }

        // Check for dangerous path components
        for component in relative_path.components() {
            match component {
                std::path::Component::ParentDir => {
                    anyhow::bail!("Path traversal attempts (..) are not allowed: {}", relative_path.display());
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
                    anyhow::bail!("Invalid path component in: {}", relative_path.display());
                }
            }
        }

        // Construct the safe target path
        let workspace = Path::new(workspace_path);
        let target_path = if relative_path.as_os_str().is_empty() || relative_path == Path::new(".") {
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

        if !target_str.starts_with(&*workspace_str) && target_path != workspace {
            anyhow::bail!("Path escapes workspace boundary: {} -> {} (workspace: {})",
                         relative_path.display(), target_path.display(), workspace_str);
        }

        Ok(target_path)
    }

    /// Validate a filename for script creation (no path separators, safe characters only)
    pub fn validate_script_name(filename: &str) -> Result<()> {
        // Check for empty name
        if filename.is_empty() {
            anyhow::bail!("Script name cannot be empty");
        }

        // Check for reasonable length
        if filename.len() > 255 {
            anyhow::bail!("Script name too long (max 255 characters): {} characters provided", filename.len());
        }

        // Check for path separators
        if filename.contains('/') || filename.contains('\\') {
            anyhow::bail!("Script name cannot contain path separators: {}", filename);
        }

        // Check for dangerous characters
        if filename.contains("..") || filename.starts_with('.') {
            anyhow::bail!("Script name cannot contain '..' or start with '.': {}", filename);
        }

        // Only allow alphanumeric, dash, underscore, and dots (for extensions)
        if !filename.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.') {
            anyhow::bail!("Script name can only contain alphanumeric characters, dashes, underscores, and dots: {}", filename);
        }

        Ok(())
    }

    /// Create a safe destination path within a restricted directory
    pub fn safe_destination_path(base_dir: &Path, filename: &str) -> Result<PathBuf> {
        // Validate the filename first
        Self::validate_script_name(filename)?;

        // Ensure base directory exists and is absolute
        if !base_dir.is_absolute() {
            anyhow::bail!("Base directory must be absolute: {}", base_dir.display());
        }

        let destination = base_dir.join(filename);

        // Double-check that we haven't escaped the base directory
        let canonical_base = base_dir.canonicalize()
            .context("Failed to canonicalize base directory")?;

        // Since destination might not exist, we check the parent
        if let Some(parent) = destination.parent() {
            if let Ok(canonical_parent) = parent.canonicalize() {
                if !canonical_parent.starts_with(&canonical_base) {
                    anyhow::bail!("Destination escapes base directory: {}", destination.display());
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
        let result = SecurityValidator::validate_script_name("../script");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".."));
    }
}