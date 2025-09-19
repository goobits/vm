//! Configuration-related error handling

use crate::{vm_error, vm_error_hint};
use std::path::Path;

/// Handle configuration file not found error
pub fn config_not_found(path: &Path) -> anyhow::Error {
    vm_error!("Configuration file '{}' not found", path.display());
    vm_error_hint!("Create one with: vm init");
    anyhow::anyhow!("Configuration file not found: {}", path.display())
}

/// Handle invalid configuration field error
pub fn invalid_field(field: &str, reason: &str) -> anyhow::Error {
    vm_error!("Invalid configuration field '{}': {}", field, reason);
    vm_error_hint!("Check the documentation for valid field formats");
    anyhow::anyhow!("Invalid field: {}", field)
}

/// Handle configuration parsing error
pub fn parse_error(path: &Path, line: Option<usize>) -> anyhow::Error {
    match line {
        Some(l) => {
            vm_error!(
                "Configuration parsing failed at line {} in {}",
                l,
                path.display()
            );
            vm_error_hint!("Check YAML syntax and indentation");
        }
        None => {
            vm_error!("Configuration parsing failed in {}", path.display());
            vm_error_hint!("Validate YAML syntax with: yamllint {}", path.display());
        }
    }
    anyhow::anyhow!("Configuration parsing failed")
}
