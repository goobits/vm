use std::path::{Path, PathBuf};
use vm_core::error::{Result, VmError};

/// Security utilities for path validation and command sanitization
pub struct SecurityValidator;

impl SecurityValidator {
    /// Validate the source directory of a VM-managed package checkout.
    pub fn validate_managed_checkout_path(path: &Path, home: &Path) -> Result<PathBuf> {
        let guest_path = path
            .to_str()
            .ok_or_else(|| VmError::Internal("Guest path must be UTF-8".into()))?;
        let guest_home = home
            .to_str()
            .ok_or_else(|| VmError::Internal("Guest home must be UTF-8".into()))?;
        if !valid_guest_absolute(guest_path) || !valid_guest_absolute(guest_home) {
            return Err(VmError::Internal(
                "Managed checkout and guest home paths must be absolute Unix paths".into(),
            ));
        }
        if guest_path.len() > 4096 {
            return Err(VmError::Internal(
                "Managed checkout path is too long".into(),
            ));
        }
        let base = format!(
            "{}/.local/share/vm/package-checkouts/",
            guest_home.trim_end_matches('/')
        );
        let relative = guest_path.strip_prefix(&base).ok_or_else(|| {
            VmError::Internal(format!(
                "Path is outside the managed checkout root: {guest_path}"
            ))
        })?;
        let components = relative.split('/').collect::<Vec<_>>();
        let valid_id = components.first().is_some_and(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
        });
        if components.len() != 2 || !valid_id || components[1] != "source" {
            return Err(VmError::Internal(format!(
                "Path is not a managed checkout source directory: {guest_path}"
            )));
        }
        Ok(path.to_path_buf())
    }

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
            return Err(VmError::Internal(format!(
                "Path too long (max 4096 characters): {} characters provided",
                path_str.len()
            )));
        }
        // These paths address a Unix guest, regardless of the host OS.
        if path_str.starts_with('/') {
            return Err(VmError::Internal(format!("Absolute paths are not allowed; use a path relative to the workspace root: {path_str}")));
        }
        if path_str.split('/').any(|part| part == "..") {
            return Err(VmError::Internal(format!(
                "Path traversal attempts (..) are not allowed: {path_str}"
            )));
        }
        if path_str.contains(['\\', ':', '\0']) || !valid_guest_absolute(workspace_path) {
            return Err(VmError::Internal(format!("Invalid guest path: {path_str}")));
        }
        let relative = path_str
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect::<Vec<_>>()
            .join("/");
        let target_path = if relative.is_empty() {
            PathBuf::from(workspace_path)
        } else {
            PathBuf::from(format!(
                "{}/{relative}",
                workspace_path.trim_end_matches('/')
            ))
        };

        Ok(target_path)
    }
}

fn valid_guest_absolute(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && !path.split('/').any(|part| part == ".." || part == ".")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn guest_paths_do_not_accept_host_windows_paths() {
        for path in [r"C:\workspace", r"..\secret", r"\server\share"] {
            assert!(
                SecurityValidator::validate_relative_path(Path::new(path), "/workspace").is_err()
            );
        }
        assert_eq!(
            SecurityValidator::validate_relative_path(Path::new("./src//file"), "/workspace/")
                .unwrap()
                .to_str(),
            Some("/workspace/src/file")
        );
        assert!(SecurityValidator::validate_managed_checkout_path(
            Path::new("/home/dev/.local/share/vm/package-checkouts/a/source"),
            Path::new("/home/dev/../dev")
        )
        .is_err());
    }

    #[test]
    fn test_validate_relative_path_normal() {
        let workspace = "/workspace";
        let relative = Path::new("src/main.rs");
        let result = SecurityValidator::validate_relative_path(relative, workspace);
        assert!(result.is_ok());
        assert_eq!(
            result.expect("Validation should succeed for a normal path"),
            Path::new("/workspace/src/main.rs")
        );
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
        assert_eq!(
            result.expect("Validation should succeed for a valid path"),
            Path::new("/home/user/documents/file.txt")
        );
    }

    #[test]
    fn managed_checkout_paths_are_narrowly_scoped() {
        let home = Path::new("/home/developer");
        assert_eq!(
            SecurityValidator::validate_managed_checkout_path(
                Path::new("/home/developer/.local/share/vm/package-checkouts/checkout-123/source"),
                home,
            )
            .unwrap(),
            Path::new("/home/developer/.local/share/vm/package-checkouts/checkout-123/source")
        );
        assert!(SecurityValidator::validate_managed_checkout_path(
            Path::new("/home/developer/.local/share/vm/package-checkouts/checkout-123/../source"),
            home,
        )
        .is_err());
        assert!(
            SecurityValidator::validate_managed_checkout_path(Path::new("/workspace"), home,)
                .is_err()
        );
    }
}
