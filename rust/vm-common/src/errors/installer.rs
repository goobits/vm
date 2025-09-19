//! Installation and build error handling

use crate::{vm_error, vm_error_hint};

/// Handle project root not found error
pub fn project_root_not_found() -> anyhow::Error {
    vm_error!("Could not find project root from executable location. Ensure the project structure is intact.");
    vm_error_hint!("Make sure you're running from within the project directory");
    anyhow::anyhow!("Could not find project root")
}

/// Handle cargo build failure
pub fn cargo_build_failed(exit_code: i32) -> anyhow::Error {
    vm_error!("Cargo build failed with exit code {}", exit_code);
    vm_error_hint!("Check build output above for compilation errors");
    anyhow::anyhow!("Cargo build failed")
}

/// Handle cargo clean failure
pub fn cargo_clean_failed(exit_code: i32) -> anyhow::Error {
    vm_error!("Cargo clean failed with exit code {}", exit_code);
    vm_error_hint!("Check file permissions in target directory");
    anyhow::anyhow!("Cargo clean failed")
}

/// Handle binary not found after build
pub fn binary_not_found(expected_path: &std::path::Path) -> anyhow::Error {
    vm_error!(
        "Built binary not found at expected location: {}",
        expected_path.display()
    );
    vm_error_hint!("Build may have failed silently, check build output");
    anyhow::anyhow!("Built binary not found")
}

/// Handle symlink creation failure
pub fn symlink_creation_failed(
    from: &std::path::Path,
    to: &std::path::Path,
    reason: &str,
) -> anyhow::Error {
    vm_error!(
        "Failed to create symlink from {} to {}: {}",
        from.display(),
        to.display(),
        reason
    );
    vm_error_hint!("Check file permissions and that target directory exists");
    anyhow::anyhow!("Symlink creation failed")
}

/// Handle installation target directory creation failure
pub fn target_dir_creation_failed(path: &std::path::Path) -> anyhow::Error {
    vm_error!(
        "Failed to create installation directory: {}",
        path.display()
    );
    vm_error_hint!("Check parent directory permissions");
    anyhow::anyhow!("Target directory creation failed")
}

/// Handle binary copy failure
pub fn binary_copy_failed(from: &std::path::Path, to: &std::path::Path) -> anyhow::Error {
    vm_error!(
        "Failed to copy binary from {} to {}",
        from.display(),
        to.display()
    );
    vm_error_hint!("Check disk space and file permissions");
    anyhow::anyhow!("Binary copy failed")
}

/// Handle PATH modification failure
pub fn path_modification_failed(reason: &str) -> anyhow::Error {
    vm_error!("Failed to modify PATH: {}", reason);
    vm_error_hint!("You may need to add the binary directory to PATH manually");
    anyhow::anyhow!("PATH modification failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_project_root_not_found() {
        let err = project_root_not_found();
        assert!(err.to_string().contains("Could not find project root"));
    }

    #[test]
    fn test_cargo_build_failed() {
        let err = cargo_build_failed(1);
        assert!(err.to_string().contains("Cargo build failed"));
    }

    #[test]
    fn test_binary_not_found() {
        let err = binary_not_found(Path::new("/test/path"));
        assert!(err.to_string().contains("Built binary not found"));
    }
}
