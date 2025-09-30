use anyhow::Result;
use std::collections::HashSet;

use crate::types::{Plugin, PluginType, PresetContent, ServiceContent};

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
    validate_metadata(plugin, &mut result)?;

    // Validate content based on plugin type
    match plugin.info.plugin_type {
        PluginType::Preset => validate_preset_content(plugin, &mut result)?,
        PluginType::Service => validate_service_content(plugin, &mut result)?,
    }

    Ok(result)
}

/// Validate plugin with semantic checks (port conflicts, etc.)
pub fn validate_plugin_with_context(plugin: &Plugin) -> Result<ValidationResult> {
    let mut result = validate_plugin(plugin)?;

    // Add semantic validation for services
    if plugin.info.plugin_type == PluginType::Service {
        validate_service_port_conflicts(plugin, &mut result)?;
    }

    Ok(result)
}

/// Check for port conflicts with other installed plugins
fn validate_service_port_conflicts(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    let content = match crate::discovery::load_service_content(plugin) {
        Ok(c) => c,
        Err(_) => return Ok(()), // Already reported in basic validation
    };

    // Get all installed plugins
    let plugins = match crate::discovery::discover_plugins() {
        Ok(p) => p,
        Err(_) => return Ok(()), // Can't check conflicts if discovery fails
    };

    let service_plugins = crate::discovery::get_service_plugins(&plugins);

    // Extract ports from this plugin
    let mut this_ports = HashSet::new();
    for port_mapping in &content.ports {
        if let Some(host_port) = extract_host_port(port_mapping) {
            this_ports.insert(host_port);
        }
    }

    // Check against other plugins
    for other_plugin in service_plugins {
        // Skip self
        if other_plugin.info.name == plugin.info.name {
            continue;
        }

        check_plugin_port_conflict(other_plugin, &this_ports, result);
    }

    Ok(())
}

/// Check for port conflicts with a specific plugin
fn check_plugin_port_conflict(
    other_plugin: &Plugin,
    this_ports: &HashSet<u16>,
    result: &mut ValidationResult,
) {
    let Ok(other_content) = crate::discovery::load_service_content(other_plugin) else {
        return;
    };

    for port_mapping in &other_content.ports {
        let Some(host_port) = extract_host_port(port_mapping) else {
            continue;
        };

        if this_ports.contains(&host_port) {
            result.add_error(
                ValidationError::new(
                    "ports",
                    format!(
                        "Port {} conflicts with existing plugin '{}'",
                        host_port, other_plugin.info.name
                    ),
                )
                .with_suggestion(format!(
                    "Change the host port to an unused port (e.g., {})",
                    find_available_port(host_port, this_ports)
                )),
            );
        }
    }
}

/// Extract host port from port mapping string
fn extract_host_port(port_mapping: &str) -> Option<u16> {
    let parts: Vec<&str> = port_mapping.split(':').collect();

    match parts.len() {
        1 => parts[0].parse::<u16>().ok(),
        2 => parts[0].parse::<u16>().ok(),
        _ => None,
    }
}

/// Find an available port near the requested port
fn find_available_port(base_port: u16, used_ports: &HashSet<u16>) -> u16 {
    for offset in 1..100 {
        let candidate = base_port.saturating_add(offset);
        if !used_ports.contains(&candidate) && candidate < 65535 {
            return candidate;
        }
    }
    base_port.saturating_add(100)
}

/// Validate plugin metadata (plugin.yaml)
fn validate_metadata(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    // Validate name
    if plugin.info.name.is_empty() {
        result.add_error(
            ValidationError::new("name", "Plugin name cannot be empty")
                .with_suggestion("Add a descriptive name like 'rust-advanced' or 'postgres-db'"),
        );
    } else if !plugin
        .info
        .name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
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
            .with_suggestion(format!("Create {} in the plugin directory", expected_file)),
        );
    }

    Ok(())
}

/// Validate preset content (preset.yaml)
fn validate_preset_content(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    let content = match crate::discovery::load_preset_content(plugin) {
        Ok(c) => c,
        Err(e) => {
            result.add_error(
                ValidationError::new(
                    "preset_content",
                    format!("Failed to parse preset.yaml: {}", e),
                )
                .with_suggestion("Check YAML syntax and structure"),
            );
            return Ok(());
        }
    };

    validate_preset_packages(&content, result);
    validate_preset_environment(&content, result);
    validate_preset_provision(&content, result);

    Ok(())
}

/// Validate service content (service.yaml)
fn validate_service_content(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    let content = match crate::discovery::load_service_content(plugin) {
        Ok(c) => c,
        Err(e) => {
            result.add_error(
                ValidationError::new(
                    "service_content",
                    format!("Failed to parse service.yaml: {}", e),
                )
                .with_suggestion("Check YAML syntax and structure"),
            );
            return Ok(());
        }
    };

    validate_service_image(&content, result);
    validate_service_ports(&content, result);
    validate_service_volumes(&content, result);
    validate_service_environment(&content, result);

    Ok(())
}

/// Validate preset packages
fn validate_preset_packages(content: &PresetContent, result: &mut ValidationResult) {
    // Check for duplicate packages
    let mut seen = HashSet::new();
    for package in &content.packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate apt package '{}' in packages list",
                package
            ));
        }
        validate_package_name(package, "packages", result);
    }

    // Check npm packages
    seen.clear();
    for package in &content.npm_packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate npm package '{}' in npm_packages list",
                package
            ));
        }
        validate_package_name(package, "npm_packages", result);
    }

    // Check pip packages
    seen.clear();
    for package in &content.pip_packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate pip package '{}' in pip_packages list",
                package
            ));
        }
        validate_package_name(package, "pip_packages", result);
    }

    // Check cargo packages
    seen.clear();
    for package in &content.cargo_packages {
        if !seen.insert(package) {
            result.add_warning(format!(
                "Duplicate cargo package '{}' in cargo_packages list",
                package
            ));
        }
        validate_package_name(package, "cargo_packages", result);
    }
}

/// Validate preset environment variables
fn validate_preset_environment(content: &PresetContent, result: &mut ValidationResult) {
    for (key, value) in &content.environment {
        // Check for valid environment variable names
        if key.is_empty() {
            result.add_error(
                ValidationError::new("environment", "Environment variable name cannot be empty")
                    .with_suggestion("Remove the empty key or provide a valid name"),
            );
        } else if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            result.add_error(
                ValidationError::new(
                    "environment",
                    format!("Invalid environment variable name: '{}'", key),
                )
                .with_suggestion("Use only alphanumeric characters and underscores"),
            );
        }

        // Warn about potentially sensitive values
        if value.to_lowercase().contains("password")
            || value.to_lowercase().contains("secret")
            || value.to_lowercase().contains("token")
        {
            result.add_warning(format!(
                "Environment variable '{}' may contain sensitive data. Consider using a placeholder.",
                key
            ));
        }
    }
}

/// Validate preset provision scripts
fn validate_preset_provision(content: &PresetContent, result: &mut ValidationResult) {
    for (i, script) in content.provision.iter().enumerate() {
        if script.trim().is_empty() {
            result.add_warning(format!(
                "Empty provision script at index {}. Consider removing it.",
                i
            ));
        }

        // Warn about potentially destructive commands
        if script.contains("rm -rf /") || script.contains("dd if=") {
            result.add_error(
                ValidationError::new(
                    "provision",
                    format!(
                        "Potentially destructive command in provision script: {}",
                        script
                    ),
                )
                .with_suggestion("Remove dangerous commands from provision scripts"),
            );
        }
    }
}

/// Validate service Docker image
fn validate_service_image(content: &ServiceContent, result: &mut ValidationResult) {
    if content.image.is_empty() {
        result.add_error(
            ValidationError::new("image", "Docker image cannot be empty")
                .with_suggestion("Specify a Docker image like 'postgres:15' or 'redis:7-alpine'"),
        );
        return;
    }

    // Check for image format (registry/image:tag or image:tag)
    if !content.image.contains(':') {
        result.add_warning(
            "Docker image does not specify a tag. Consider using a specific version tag."
                .to_string(),
        );
    }

    // Warn about 'latest' tag
    if content.image.ends_with(":latest") {
        result.add_warning(
            "Using 'latest' tag is not recommended. Pin to a specific version for reproducibility."
                .to_string(),
        );
    }
}

/// Validate service ports
fn validate_service_ports(content: &ServiceContent, result: &mut ValidationResult) {
    for port in &content.ports {
        validate_port_mapping(port, result);
    }
}

/// Validate service volumes
fn validate_service_volumes(content: &ServiceContent, result: &mut ValidationResult) {
    for volume in &content.volumes {
        // Check volume format (source:target or named_volume:target)
        if !volume.contains(':') {
            result.add_error(
                ValidationError::new("volumes", format!("Invalid volume format: '{}'", volume))
                    .with_suggestion("Use format 'source:target' or 'volume_name:target'"),
            );
        }
    }
}

/// Validate service environment variables
fn validate_service_environment(content: &ServiceContent, result: &mut ValidationResult) {
    for (key, value) in &content.environment {
        // Check for valid environment variable names
        if key.is_empty() {
            result.add_error(
                ValidationError::new("environment", "Environment variable name cannot be empty")
                    .with_suggestion("Remove the empty key or provide a valid name"),
            );
        } else if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            result.add_error(
                ValidationError::new(
                    "environment",
                    format!("Invalid environment variable name: '{}'", key),
                )
                .with_suggestion("Use only alphanumeric characters and underscores"),
            );
        }

        // Warn about potentially sensitive values
        if value.to_lowercase().contains("password")
            || value.to_lowercase().contains("secret")
            || value.to_lowercase().contains("token")
        {
            result.add_warning(format!(
                "Environment variable '{}' may contain sensitive data. Consider using a placeholder.",
                key
            ));
        }
    }
}

/// Validate port mapping format
fn validate_port_mapping(port: &str, result: &mut ValidationResult) {
    let parts: Vec<&str> = port.split(':').collect();

    match parts.len() {
        1 => {
            // Container port only
            if let Err(e) = parts[0].parse::<u16>() {
                result.add_error(
                    ValidationError::new(
                        "ports",
                        format!("Invalid port number: '{}' - {}", parts[0], e),
                    )
                    .with_suggestion("Use a valid port number (1-65535)"),
                );
            }
        }
        2 => {
            // Host:container port mapping
            if let Err(e) = parts[0].parse::<u16>() {
                result.add_error(
                    ValidationError::new(
                        "ports",
                        format!("Invalid host port: '{}' - {}", parts[0], e),
                    )
                    .with_suggestion("Use a valid port number (1-65535)"),
                );
            }
            if let Err(e) = parts[1].parse::<u16>() {
                result.add_error(
                    ValidationError::new(
                        "ports",
                        format!("Invalid container port: '{}' - {}", parts[1], e),
                    )
                    .with_suggestion("Use a valid port number (1-65535)"),
                );
            }
        }
        _ => {
            result.add_error(
                ValidationError::new("ports", format!("Invalid port mapping format: '{}'", port))
                    .with_suggestion("Use format 'port' or 'host_port:container_port'"),
            );
        }
    }
}

/// Validate package name format
fn validate_package_name(name: &str, field: &str, result: &mut ValidationResult) {
    if name.trim().is_empty() {
        result.add_error(
            ValidationError::new(field, "Package name cannot be empty")
                .with_suggestion("Remove empty entries from package list"),
        );
    } else if name.contains(' ') {
        result.add_error(
            ValidationError::new(field, format!("Package name '{}' contains spaces", name))
                .with_suggestion("Package names should not contain spaces"),
        );
    }
}

/// Check if string is valid semver format
fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();

    if parts.len() != 3 {
        return false;
    }

    parts.iter().all(|part| part.parse::<u32>().is_ok())
}

#[cfg(test)]
mod tests {
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
