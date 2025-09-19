//! Package management error handling

use crate::{vm_error, vm_error_hint};

/// Handle invalid script name error
pub fn invalid_script_name(reason: &str) -> anyhow::Error {
    vm_error!("Script name validation failed: {}", reason);
    vm_error_hint!("Use only alphanumeric characters, dashes, and underscores");
    anyhow::anyhow!("Invalid script name")
}

/// Handle empty script name error
pub fn empty_script_name() -> anyhow::Error {
    vm_error!("Script name cannot be empty");
    vm_error_hint!("Provide a valid script name");
    anyhow::anyhow!("Script name cannot be empty")
}

/// Handle script name with path separators
pub fn script_name_has_path_separators(filename: &str) -> anyhow::Error {
    vm_error!("Script name cannot contain path separators: {}", filename);
    vm_error_hint!("Remove any '/' or '\\' characters from the script name");
    anyhow::anyhow!("Script name cannot contain path separators")
}

/// Handle script name with dangerous characters
pub fn script_name_has_dangerous_chars(filename: &str) -> anyhow::Error {
    vm_error!(
        "Script name cannot contain '..' or start with '.': {}",
        filename
    );
    vm_error_hint!("Remove any '..' patterns and avoid starting with '.'");
    anyhow::anyhow!("Script name cannot contain '..' or start with '.'")
}

/// Handle script name with invalid characters
pub fn script_name_invalid_chars(filename: &str) -> anyhow::Error {
    vm_error!(
        "Script name can only contain alphanumeric characters, dashes, and underscores: {}",
        filename
    );
    vm_error_hint!("Use only letters, numbers, '-', and '_' characters");
    anyhow::anyhow!("Script name can only contain alphanumeric characters, dashes, and underscores")
}

/// Handle package manager not available error
pub fn package_manager_unavailable(manager: &dyn std::fmt::Display) -> anyhow::Error {
    vm_error!("Package manager {} is not available", manager);
    vm_error_hint!("Install {} or ensure it's in your PATH", manager);
    anyhow::anyhow!("Package manager not available")
}

/// Handle package installation failure
pub fn package_install_failed(package: &str, manager: &dyn std::fmt::Display) -> anyhow::Error {
    vm_error!("Failed to install package '{}' using {}", package, manager);
    vm_error_hint!("Check package name and network connectivity");
    anyhow::anyhow!("Package installation failed")
}

/// Handle cargo install failure for linked package
pub fn cargo_install_failed_linked(package: &str) -> anyhow::Error {
    vm_error!("Cargo install failed for linked package: {}", package);
    vm_error_hint!("Check that the local path is correct and the package builds successfully");
    anyhow::anyhow!("Cargo install failed")
}

/// Handle cargo install failure for registry package
pub fn cargo_install_failed_registry(package: &str) -> anyhow::Error {
    vm_error!("Cargo install failed for package: {}", package);
    vm_error_hint!("Check package name and ensure cargo registry is accessible");
    anyhow::anyhow!("Cargo install failed")
}

/// Handle npm link failure
pub fn npm_link_failed(package: &str) -> anyhow::Error {
    vm_error!("NPM link failed for package: {}", package);
    vm_error_hint!("Check that the local package directory exists and npm link permissions");
    anyhow::anyhow!("NPM link failed")
}

/// Handle npm install failure
pub fn npm_install_failed(package: &str) -> anyhow::Error {
    vm_error!("NPM install failed for package: {}", package);
    vm_error_hint!("Check package name and npm registry connectivity");
    anyhow::anyhow!("NPM install failed")
}

/// Handle pip install failure
pub fn pip_install_failed(package: &str) -> anyhow::Error {
    vm_error!("Pip install failed for package: {}", package);
    vm_error_hint!("Check package name and PyPI connectivity");
    anyhow::anyhow!("Pip install failed")
}

/// Handle pip editable install failure
pub fn pip_editable_install_failed() -> anyhow::Error {
    vm_error!("Pip editable install failed");
    vm_error_hint!("Check that the local directory contains a valid setup.py or pyproject.toml");
    anyhow::anyhow!("Pip editable install failed")
}

/// Handle pipx install failure
pub fn pipx_install_failed(stderr: &str) -> anyhow::Error {
    vm_error!("Pipx install failed: {}", stderr);
    vm_error_hint!("Check package name and that pipx is properly configured");
    anyhow::anyhow!("Pipx install failed")
}

/// Handle package link operation failure
pub fn package_link_failed(package: &str, reason: &str) -> anyhow::Error {
    vm_error!("Failed to link package '{}': {}", package, reason);
    vm_error_hint!("Check file permissions and target directory access");
    anyhow::anyhow!("Package link operation failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_script_name() {
        let err = empty_script_name();
        assert!(err.to_string().contains("Script name cannot be empty"));
    }

    #[test]
    fn test_script_name_has_path_separators() {
        let err = script_name_has_path_separators("test/script");
        assert!(err
            .to_string()
            .contains("Script name cannot contain path separators"));
    }

    #[test]
    fn test_package_manager_unavailable() {
        struct TestManager;
        impl std::fmt::Display for TestManager {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "npm")
            }
        }
        let manager = TestManager;
        let err = package_manager_unavailable(&manager);
        assert!(err.to_string().contains("Package manager not available"));
    }

    #[test]
    fn test_cargo_install_failed_linked() {
        let err = cargo_install_failed_linked("test-package");
        assert!(err.to_string().contains("Cargo install failed"));
    }

    #[test]
    fn test_npm_link_failed() {
        let err = npm_link_failed("test-package");
        assert!(err.to_string().contains("NPM link failed"));
    }

    #[test]
    fn test_pip_install_failed() {
        let err = pip_install_failed("test-package");
        assert!(err.to_string().contains("Pip install failed"));
    }
}
