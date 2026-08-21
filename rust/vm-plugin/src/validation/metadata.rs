use anyhow::Result;

use crate::types::{Plugin, PluginType};

use super::{ValidationError, ValidationResult};

pub(super) fn validate(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    // Validate name
    if plugin.info.name.is_empty() {
        result.add_error(
            ValidationError::new("name", "Plugin name cannot be empty")
                .with_suggestion("Add a descriptive name like 'rust-advanced' or 'postgres-db'"),
        );
    } else if !is_valid_plugin_name(&plugin.info.name) {
        result.add_error(
            ValidationError::new("name", "Plugin name contains invalid characters")
                .with_suggestion("Use only alphanumeric characters, hyphens, and underscores"),
        );
    }

    // Validate version (semver format)
    if plugin.info.version.is_empty() {
        result.add_error(
            ValidationError::new("version", "Version cannot be empty")
                .with_suggestion("Use semantic versioning like '1.0.0'"),
        );
    } else if !is_valid_semver(&plugin.info.version) {
        result.add_error(
            ValidationError::new(
                "version",
                format!("Invalid version format: {}", plugin.info.version),
            )
            .with_suggestion("Use semantic versioning format: MAJOR.MINOR.PATCH (e.g., '1.0.0')"),
        );
    }

    // Validate description (recommended)
    if plugin.info.description.is_none() {
        result.add_warning(
            "No description provided. Add a description to help users understand the plugin's purpose.".to_string()
        );
    }

    // Validate author (recommended)
    if plugin.info.author.is_none() {
        result.add_warning("No author provided. Consider adding author information.".to_string());
    }

    // Validate content file exists
    if !plugin.content_file.exists() {
        let expected_file = match plugin.info.plugin_type {
            PluginType::Preset => "preset.yaml",
            PluginType::Service => "service.yaml",
        };
        result.add_error(
            ValidationError::new(
                "content_file",
                format!("Content file not found: {:?}", plugin.content_file),
            )
            .with_suggestion(format!("Create {expected_file} in the plugin directory")),
        );
    }

    Ok(())
}

/// Return whether a plugin name is safe to use as its on-disk directory name.
pub fn is_valid_plugin_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_alphanumeric() || matches!(character, '-' | '_'))
}

pub(super) fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();

    if parts.len() != 3 {
        return false;
    }

    parts.iter().all(|part| part.parse::<u32>().is_ok())
}
