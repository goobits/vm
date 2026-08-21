use anyhow::Result;
use std::collections::HashMap;

use crate::types::{Plugin, PluginType};

mod metadata;
mod preset;
mod service;

pub use metadata::is_valid_plugin_name;

/// Validation error with actionable fix suggestion
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub fix_suggestion: Option<String>,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
            fix_suggestion: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.fix_suggestion = Some(suggestion.into());
        self
    }
}

/// Result of plugin validation
#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a plugin's metadata and content
pub fn validate_plugin(plugin: &Plugin) -> Result<ValidationResult> {
    let mut result = ValidationResult::new();

    // Validate metadata
    metadata::validate(plugin, &mut result)?;

    // Validate content based on plugin type
    match plugin.info.plugin_type {
        PluginType::Preset => preset::validate(plugin, &mut result)?,
        PluginType::Service => service::validate(plugin, &mut result)?,
    }

    Ok(result)
}

/// Validate plugin with semantic checks (port conflicts, etc.)
pub fn validate_plugin_with_context(plugin: &Plugin) -> Result<ValidationResult> {
    let mut result = validate_plugin(plugin)?;

    // Add semantic validation for services
    if plugin.info.plugin_type == PluginType::Service {
        service::validate_port_conflicts(plugin, &mut result)?;
    }

    Ok(result)
}

fn validate_environment(environment: &HashMap<String, String>, result: &mut ValidationResult) {
    for (key, value) in environment {
        if key.is_empty() {
            result.add_error(
                ValidationError::new("environment", "Environment variable name cannot be empty")
                    .with_suggestion("Remove the empty key or provide a valid name"),
            );
        } else if !key
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
        {
            result.add_error(
                ValidationError::new(
                    "environment",
                    format!("Invalid environment variable name: '{key}'"),
                )
                .with_suggestion("Use only alphanumeric characters and underscores"),
            );
        }

        let value = value.to_lowercase();
        if ["password", "secret", "token"]
            .iter()
            .any(|marker| value.contains(marker))
        {
            result.add_warning(format!(
                "Environment variable '{key}' may contain sensitive data. Consider using a placeholder."
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::metadata::is_valid_semver;
    use super::service::validate_port_mapping;
    use super::*;
    use crate::types::{PluginInfo, PluginType};
    use std::fs;
    use tempfile::TempDir;

    fn create_test_preset_plugin(dir: &TempDir, name: &str, version: &str) -> Result<Plugin> {
        let plugin_dir = dir.path().join(name);
        fs::create_dir_all(&plugin_dir)?;

        let info = PluginInfo {
            name: name.to_string(),
            version: version.to_string(),
            description: Some("Test plugin".to_string()),
            author: Some("Test Author".to_string()),
            plugin_type: PluginType::Preset,
            preset_category: None,
        };

        let info_content = serde_yaml_ng::to_string(&info)?;
        fs::write(plugin_dir.join("plugin.yaml"), info_content)?;

        let preset_content = r#"
packages:
  - curl
  - git
npm_packages:
  - typescript
environment:
  TEST_VAR: "test_value"
"#;
        fs::write(plugin_dir.join("preset.yaml"), preset_content)?;

        Ok(Plugin {
            info,
            content_file: plugin_dir.join("preset.yaml"),
        })
    }

    #[test]
    fn test_valid_preset_plugin() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let plugin = create_test_preset_plugin(&temp_dir, "test-plugin", "1.0.0")?;

        let result = validate_plugin(&plugin)?;
        assert!(result.is_valid);
        assert_eq!(result.errors.len(), 0);

        Ok(())
    }

    #[test]
    fn test_invalid_plugin_name() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let plugin = create_test_preset_plugin(&temp_dir, "invalid name!", "1.0.0")?;

        let result = validate_plugin(&plugin)?;
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "name"));

        Ok(())
    }

    #[test]
    fn plugin_directory_names_reject_path_components() {
        assert!(is_valid_plugin_name("rust-tools"));
        for name in ["", ".", "..", "../presets", "/tmp/plugin", "scope/plugin"] {
            assert!(!is_valid_plugin_name(name));
        }
    }

    #[test]
    fn test_invalid_version_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let plugin = create_test_preset_plugin(&temp_dir, "test-plugin", "1.0")?;

        let result = validate_plugin(&plugin)?;
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.field == "version"));

        Ok(())
    }

    #[test]
    fn test_missing_description_warning() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let mut plugin = create_test_preset_plugin(&temp_dir, "test-plugin", "1.0.0")?;
        plugin.info.description = None;

        let result = validate_plugin(&plugin)?;
        assert!(result.is_valid);
        assert!(result.warnings.iter().any(|w| w.contains("description")));

        Ok(())
    }

    #[test]
    fn test_semver_validation() {
        assert!(is_valid_semver("1.0.0"));
        assert!(is_valid_semver("0.1.0"));
        assert!(is_valid_semver("10.20.30"));
        assert!(!is_valid_semver("1.0"));
        assert!(!is_valid_semver("1"));
        assert!(!is_valid_semver("1.0.0.0"));
        assert!(!is_valid_semver("v1.0.0"));
        assert!(!is_valid_semver("1.0.x"));
    }

    #[test]
    fn test_port_validation() {
        let mut result = ValidationResult::new();

        validate_port_mapping("8080", &mut result);
        assert_eq!(result.errors.len(), 0);

        validate_port_mapping("8080:80", &mut result);
        assert_eq!(result.errors.len(), 0);

        validate_port_mapping("invalid", &mut result);
        assert!(!result.errors.is_empty());

        result = ValidationResult::new();
        validate_port_mapping("99999", &mut result);
        assert!(!result.errors.is_empty());
    }
}
