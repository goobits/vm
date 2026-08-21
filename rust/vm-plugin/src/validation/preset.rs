use std::collections::HashSet;

use anyhow::Result;

use crate::types::{Plugin, PresetContent};

use super::{ValidationError, ValidationResult};

pub(super) fn validate(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    let content = match crate::discovery::load_preset_content(plugin) {
        Ok(c) => c,
        Err(e) => {
            result.add_error(
                ValidationError::new(
                    "preset_content",
                    format!("Failed to parse preset.yaml: {e}"),
                )
                .with_suggestion("Check YAML syntax and structure"),
            );
            return Ok(());
        }
    };

    validate_preset_packages(&content, result);
    super::validate_environment(&content.environment, result);
    validate_preset_provision(&content, result);

    Ok(())
}

fn validate_preset_packages(content: &PresetContent, result: &mut ValidationResult) {
    // Check for duplicate packages
    let mut seen = HashSet::new();
    for package in &content.packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate apt package '{package}' in packages list"
            ));
        }
        validate_package_name(package, "packages", result);
    }

    // Check npm packages
    seen.clear();
    for package in &content.npm_packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate npm package '{package}' in npm_packages list"
            ));
        }
        validate_package_name(package, "npm_packages", result);
    }

    // Check pip packages
    seen.clear();
    for package in &content.pip_packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate pip package '{package}' in pip_packages list"
            ));
        }
        validate_package_name(package, "pip_packages", result);
    }

    // Check cargo packages
    seen.clear();
    for package in &content.cargo_packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate cargo package '{package}' in cargo_packages list"
            ));
        }
        validate_package_name(package, "cargo_packages", result);
    }
}

/// Validate preset environment variables
/// Validate preset provision scripts
fn validate_preset_provision(content: &PresetContent, result: &mut ValidationResult) {
    for (i, script) in content.provision.iter().enumerate() {
        if script.trim().is_empty() {
            result.add_warning(format!(
                "Empty provision script at index {i}. Consider removing it."
            ));
        }

        // Warn about potentially destructive commands
        if script.contains("rm -rf /") || script.contains("dd if=") {
            result.add_error(
                ValidationError::new(
                    "provision",
                    format!("Potentially destructive command in provision script: {script}"),
                )
                .with_suggestion("Remove dangerous commands from provision scripts"),
            );
        }
    }
}

fn validate_package_name(name: &str, field: &str, result: &mut ValidationResult) {
    if name.trim().is_empty() {
        result.add_error(
            ValidationError::new(field, "Package name cannot be empty")
                .with_suggestion("Remove empty entries from package list"),
        );
    } else if name.contains(' ') {
        result.add_error(
            ValidationError::new(field, format!("Package name '{name}' contains spaces"))
                .with_suggestion("Package names should not contain spaces"),
        );
    }
}
