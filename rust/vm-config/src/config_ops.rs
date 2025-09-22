//! Configuration management operations for VM Tool.
//!
//! This module provides high-level operations for managing VM configuration files,
//! including setting, getting, and unsetting configuration values using dot notation,
//! applying presets, and managing configuration state with dry-run capabilities.
//! It serves as the core configuration manipulation engine for the CLI.

// Standard library
use std::fs;
use std::path::PathBuf;

// External crates
use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_yaml::{Mapping, Value};
use serde_yaml_ng as serde_yaml;
use std::sync::OnceLock;

// Internal imports
use crate::config::VmConfig;
use crate::merge::ConfigMerger;
use crate::paths;
use crate::ports::PortRange;
use crate::preset::{PresetDetector, PresetFile};
use crate::yaml::core::CoreOperations;
use vm_common::user_paths;
use vm_common::{vm_error, vm_error_hint, vm_warning};

static PORT_PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();

fn get_port_placeholder_regex() -> &'static Regex {
    PORT_PLACEHOLDER_RE.get_or_init(|| {
        // This regex pattern is known to be valid, but we handle the error case gracefully
        match Regex::new(r"\$\{port\.(\d+)\}") {
            Ok(regex) => regex,
            Err(_) => {
                // Fallback to a regex that matches nothing if the main pattern fails
                // This is extremely unlikely to fail, but if it does, use a literal pattern
                Regex::new(r"never_matches_anything_specific_placeholder_12345").unwrap_or_else(
                    |_| {
                        // Last resort - create a basic regex that will always work
                        Regex::new(r"a^").unwrap() // matches nothing (a followed by start of string)
                    },
                )
            }
        }
    })
}

/// Configuration operations for VM configuration management.
///
/// Provides high-level operations for reading, writing, and manipulating
/// VM configuration files. Supports both local project configurations
/// and global user settings.
///
/// ## Supported Operations
/// - **Set**: Modify configuration values using dot notation
/// - **Get**: Retrieve configuration values or entire configurations
/// - **Unset**: Remove configuration fields
/// - **Clear**: Reset entire configuration files
///
/// ## Configuration Scopes
/// - **Local**: Project-specific configuration (vm.yaml in project directory)
/// - **Global**: User-wide configuration (~/.config/vm/global.yaml)
///
/// # Examples
/// ```rust,no_run
/// use vm_config::ConfigOps;
///
/// // Set a local configuration value
/// ConfigOps::set("vm.memory", "4096", false, false)?;
///
/// // Get a global configuration value
/// ConfigOps::get(Some("project.name"), true)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct ConfigOps;

impl ConfigOps {
    /// Set a configuration value using dot notation.
    ///
    /// Modifies a configuration field in either the local project configuration
    /// or the global user configuration. The field path uses dot notation to
    /// access nested configuration values (e.g., "vm.memory", "services.postgresql.port").
    ///
    /// ## Value Parsing
    /// Values are parsed in the following order:
    /// 1. **YAML**: Attempts to parse as YAML (supports complex types)
    /// 2. **String**: Falls back to string value if YAML parsing fails
    ///
    /// This allows setting primitive values, arrays, and objects:
    /// - Numbers: "4096", "2.5"
    /// - Booleans: "true", "false"
    /// - Arrays: "[item1, item2]"
    /// - Objects: "{key: value}"
    ///
    /// # Arguments
    /// * `field` - Dot-notation path to the field (e.g., "vm.memory")
    /// * `value` - String representation of the value to set
    /// * `global` - If true, modifies global config; if false, modifies local config
    ///
    /// # Returns
    /// `Ok(())` if the value was set successfully
    ///
    /// # Errors
    /// Returns an error if:
    /// - Configuration file cannot be read or written
    /// - Field path is malformed
    /// - File system permissions prevent modification
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::ConfigOps;
    ///
    /// // Set VM memory
    /// ConfigOps::set("vm.memory", "4096", false, false)?;
    ///
    /// // Enable a service globally
    /// ConfigOps::set("services.postgresql.enabled", "true", true, false)?;
    ///
    /// // Set an array value
    /// ConfigOps::set("npm_packages", "[eslint, prettier]", false, false)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set(field: &str, value: &str, global: bool, dry_run: bool) -> Result<()> {
        let config_path = if global {
            get_or_create_global_config_path()?
        } else {
            find_or_create_local_config()?
        };

        let mut yaml_value = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            serde_yaml::from_str(&content)?
        } else {
            Value::Mapping(Mapping::new())
        };

        // Parse the value - try as YAML first, then as string
        let parsed_value: Value =
            serde_yaml::from_str(value).unwrap_or_else(|_| Value::String(value.into()));

        set_nested_field(&mut yaml_value, field, parsed_value)?;

        if dry_run {
            println!(
                "🔍 DRY RUN - Would set {} = {} in {}",
                field,
                value,
                config_path.display()
            );
            println!("   (no changes were made to the file)");
        } else {
            CoreOperations::write_yaml_file(&config_path, &yaml_value)?;
            println!("✅ Set {} = {} in {}", field, value, config_path.display());
        }
        Ok(())
    }

    /// Get a configuration value or display entire configuration.
    ///
    /// Retrieves and displays configuration values from either local or global
    /// configuration files. If no field is specified, displays the entire
    /// configuration in YAML format.
    ///
    /// ## Output Format
    /// - **Specific field**: Displays just the field value
    /// - **Entire config**: Pretty-printed YAML of the full configuration
    /// - **Missing field**: Shows an informative message
    ///
    /// # Arguments
    /// * `field` - Optional dot-notation path to specific field
    /// * `global` - If true, reads from global config; if false, reads from local config
    ///
    /// # Returns
    /// `Ok(())` after displaying the requested information
    ///
    /// # Errors
    /// Returns an error if:
    /// - Configuration file cannot be read
    /// - Configuration file contains invalid YAML
    /// - File system permissions prevent access
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::ConfigOps;
    ///
    /// // Get specific field
    /// ConfigOps::get(Some("vm.memory"), false)?;
    ///
    /// // Display entire local configuration
    /// ConfigOps::get(None, false)?;
    ///
    /// // Get global setting
    /// ConfigOps::get(Some("project.name"), true)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn get(field: Option<&str>, global: bool) -> Result<()> {
        let config_path = if global {
            get_global_config_path()
        } else {
            find_local_config()?
        };

        if !config_path.exists() {
            if global {
                vm_error!("No global configuration found at {}", config_path.display());
                vm_error_hint!("Global configs are created automatically when needed");
                return Err(anyhow::anyhow!(
                    "No global configuration found at '{}'. Global configuration is created automatically when needed",
                    config_path.display()
                ));
            } else {
                vm_error!("No vm.yaml found in current directory or parent directories");
                vm_error_hint!("Create one with: vm init");
                return Err(anyhow::anyhow!(
                    "No vm.yaml found in current directory or parent directories. Create one with: vm init"
                ));
            }
        }

        let content = fs::read_to_string(&config_path)?;
        let yaml_value: Value = serde_yaml::from_str(&content)?;

        match field {
            Some(f) => {
                // Get specific field
                let value = get_nested_field(&yaml_value, f)?;
                match value {
                    Value::String(s) => println!("{}", s),
                    _ => println!("{}", serde_yaml::to_string(value)?),
                }
            }
            None => {
                // Show all configuration
                println!("{}", serde_yaml::to_string(&yaml_value)?);
            }
        }

        Ok(())
    }

    /// Unset (remove) a configuration field
    pub fn unset(field: &str, global: bool) -> Result<()> {
        let config_path = if global {
            get_global_config_path()
        } else {
            find_local_config()?
        };

        if !config_path.exists() {
            vm_error!("Configuration file not found: {}", config_path.display());
            return Err(anyhow::anyhow!(
                "Configuration file not found at '{}'. Use 'vm init' to create a configuration",
                config_path.display()
            ));
        }

        let content = fs::read_to_string(&config_path)?;
        let mut yaml_value: Value = serde_yaml::from_str(&content)?;

        unset_nested_field(&mut yaml_value, field)?;

        CoreOperations::write_yaml_file(&config_path, &yaml_value)?;

        println!("✅ Unset {} in {}", field, config_path.display());
        Ok(())
    }

    /// Apply preset(s) to configuration
    pub fn preset(preset_names: &str, global: bool, list: bool, show: Option<&str>) -> Result<()> {
        let presets_dir = paths::get_presets_dir();
        let project_dir = std::env::current_dir()?;
        let detector = PresetDetector::new(project_dir, presets_dir);

        // Handle list command
        if list {
            let presets = detector.list_presets()?;
            println!("Available presets:");
            for preset in presets {
                println!("  - {}", preset);
            }
            return Ok(());
        }

        // Handle show command
        if let Some(name) = show {
            let preset_config = detector.load_preset(name)?;
            let yaml = serde_yaml::to_string(&preset_config)?;
            println!("Preset '{}' configuration:", name);
            println!("{}", yaml);
            return Ok(());
        }

        // Apply preset(s)
        let config_path = if global {
            get_or_create_global_config_path()?
        } else {
            find_or_create_local_config()?
        };

        // Load existing config or create empty
        let base_config = if config_path.exists() {
            let content = fs::read_to_string(&config_path)?;
            serde_yaml::from_str(&content)?
        } else {
            VmConfig::default()
        };

        // Parse comma-separated presets and merge them - avoid Vec allocation
        let preset_iter = preset_names.split(',').map(|s| s.trim());
        let mut merged_config = base_config;

        for preset_name in preset_iter {
            // Get port_range from the base config
            let port_range = merged_config.port_range.clone();

            // Load preset with optimized placeholder replacement
            let preset_config = load_preset_with_placeholders(&detector, preset_name, &port_range)
                .with_context(|| format!("Failed to load preset: {}", preset_name))?;

            merged_config = ConfigMerger::new(merged_config).merge(preset_config)?;
        }

        // Save merged configuration with consistent formatting
        // Convert VmConfig to Value first
        let config_yaml = serde_yaml::to_string(&merged_config)?;
        let config_value: Value = serde_yaml::from_str(&config_yaml)?;
        CoreOperations::write_yaml_file(&config_path, &config_value)?;

        let scope = if global { "global" } else { "local" };
        println!(
            "✅ Applied preset(s) {} to {} configuration",
            preset_names, scope
        );
        Ok(())
    }
}

// Helper functions

fn replace_port_placeholders(value: &mut Value, port_range_str: &Option<String>) {
    let Some(port_range_str) = port_range_str.as_ref() else {
        return;
    };

    let port_range = match PortRange::parse(port_range_str) {
        Ok(range) => range,
        Err(_) => {
            vm_warning!("Could not parse port_range '{}'", port_range_str);
            return;
        }
    };

    replace_placeholders_recursive(value, &port_range);
}

/// Extract port number from placeholder string, using early returns to reduce nesting
fn extract_port_from_placeholder(s: &str, port_range: &PortRange) -> Option<u16> {
    let captures = get_port_placeholder_regex().captures(s)?;
    let index_match = captures.get(1)?;
    let index = index_match.as_str().parse::<u16>().ok()?;

    if index >= port_range.size() {
        vm_warning!(
            "Port index {} is out of bounds for the allocated range",
            index
        );
        return None;
    }

    Some(port_range.start + index)
}

fn replace_placeholders_recursive(value: &mut Value, port_range: &PortRange) {
    match value {
        Value::Mapping(mapping) => {
            for (_, val) in mapping.iter_mut() {
                replace_placeholders_recursive(val, port_range);
            }
        }
        Value::Sequence(sequence) => {
            for val in sequence.iter_mut() {
                replace_placeholders_recursive(val, port_range);
            }
        }
        Value::String(s) => {
            if let Some(port_value) = extract_port_from_placeholder(s, port_range) {
                *value = Value::Number(port_value.into());
            }
        }
        _ => {}
    }
}

/// Optimized preset loading with placeholder replacement
/// This avoids the expensive serialize → modify → deserialize cycle by doing string replacement
/// before YAML parsing
fn load_preset_with_placeholders(
    detector: &PresetDetector,
    preset_name: &str,
    port_range_str: &Option<String>,
) -> Result<VmConfig> {
    // First try to load the preset normally to get the raw content
    // We'll use the same logic as PresetDetector::load_preset but with string processing
    let raw_content =
        if let Some(content) = crate::embedded_presets::get_preset_content(preset_name) {
            content.to_string()
        } else {
            // For filesystem presets, we'd need access to presets_dir which is private
            // For now, fall back to the original method and then do the replacement
            // This is still more efficient than the original serialize/deserialize cycle
            let original_config = detector.load_preset(preset_name)?;

            // If no port_range, return the original config
            let Some(port_range_str) = port_range_str else {
                return Ok(original_config);
            };

            // For non-embedded presets, we fall back to the Value-based approach
            // but this is still better than before since embedded presets (most common) are optimized
            let mut preset_value = serde_yaml::to_value(&original_config)?;
            replace_port_placeholders(&mut preset_value, &Some(port_range_str.clone()));
            return Ok(serde_yaml::from_value(preset_value)?);
        };

    // Replace placeholders in the raw string if port_range is available
    let processed_content = if let Some(port_range_str) = port_range_str {
        replace_placeholders_in_string(&raw_content, port_range_str)?
    } else {
        raw_content
    };

    // Parse the processed content directly to VmConfig
    let preset_file: PresetFile = serde_yaml::from_str(&processed_content)
        .with_context(|| format!("Failed to parse preset '{}'", preset_name))?;

    Ok(preset_file.config)
}

/// Replace placeholders in raw YAML string - much more efficient than serialize/deserialize
fn replace_placeholders_in_string(content: &str, port_range_str: &str) -> Result<String> {
    let port_range = match PortRange::parse(port_range_str) {
        Ok(range) => range,
        Err(_) => {
            vm_warning!("Could not parse port_range '{}'", port_range_str);
            return Ok(content.to_string());
        }
    };

    let mut result = content.to_string();

    // Use regex to find and replace all ${port.N} placeholders
    let regex = get_port_placeholder_regex();

    // We need to be careful about replacing in the correct order to avoid conflicts
    // Collect all matches first, then replace them
    let mut replacements = Vec::new();

    for capture in regex.captures_iter(content) {
        if let (Some(full_match), Some(index_match)) = (capture.get(0), capture.get(1)) {
            if let Ok(index) = index_match.as_str().parse::<u16>() {
                if index < port_range.size() {
                    let port_value = port_range.start + index;
                    replacements.push((full_match.as_str().to_string(), port_value.to_string()));
                } else {
                    vm_warning!(
                        "Port index {} is out of bounds for the allocated range",
                        index
                    );
                }
            }
        }
    }

    // Apply replacements
    for (placeholder, replacement) in replacements {
        result = result.replace(&placeholder, &replacement);
    }

    Ok(result)
}

fn get_global_config_path() -> PathBuf {
    // Check for test environment override first
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("vm")
            .join("global.yaml");
    }

    user_paths::global_config_path().unwrap_or_else(|_| PathBuf::from(".config/vm/global.yaml"))
}

fn get_or_create_global_config_path() -> Result<PathBuf> {
    let config_dir = if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("vm")
    } else {
        user_paths::user_config_dir()?
    };

    if !config_dir.exists() {
        fs::create_dir_all(&config_dir).with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;
    }

    Ok(config_dir.join("global.yaml"))
}

fn find_local_config() -> Result<PathBuf> {
    let current_dir = std::env::current_dir()?;
    let mut dir = current_dir.as_path();

    loop {
        let config_path = dir.join("vm.yaml");
        if config_path.exists() {
            return Ok(config_path);
        }

        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    vm_error!("No vm.yaml found in current directory or parent directories");
    Err(anyhow::anyhow!(
        "No vm.yaml configuration found in current directory or parent directories. Create one with: vm init"
    ))
}

fn find_or_create_local_config() -> Result<PathBuf> {
    // First try to find existing config
    if let Ok(path) = find_local_config() {
        return Ok(path);
    }

    // If not found, create in current directory
    let current_dir = std::env::current_dir()?;
    Ok(current_dir.join("vm.yaml"))
}

fn set_nested_field(value: &mut Value, field: &str, new_value: Value) -> Result<()> {
    if field.is_empty() {
        vm_error!("Empty field path");
        return Err(anyhow::anyhow!(
            "Empty field path provided. Specify a field name like 'provider' or 'project.name'"
        ));
    }

    let parts: Vec<&str> = field.split('.').collect();
    set_nested_field_recursive(value, &parts, new_value)
}

fn set_nested_field_recursive(value: &mut Value, parts: &[&str], new_value: Value) -> Result<()> {
    if parts.len() == 1 {
        match value {
            Value::Mapping(map) => {
                let key = Value::String(parts[0].into());
                map.insert(key, new_value);
                return Ok(());
            }
            _ => {
                vm_error!("Cannot set field on non-object");
                return Err(anyhow::anyhow!(
                    "Cannot set field '{}' on non-object value. Field path may be invalid",
                    parts[0]
                ));
            }
        }
    }

    match value {
        Value::Mapping(map) => {
            let key = Value::String(parts[0].into());
            match map.get_mut(&key) {
                Some(nested) => set_nested_field_recursive(nested, &parts[1..], new_value)?,
                None => {
                    // Create nested structure
                    let mut nested = Value::Mapping(Mapping::new());
                    set_nested_field_recursive(&mut nested, &parts[1..], new_value)?;
                    map.insert(key, nested);
                }
            }
        }
        _ => {
            vm_error!("Cannot navigate field '{}' on non-object", parts[0]);
            return Err(anyhow::anyhow!("Cannot navigate field on non-object"));
        }
    }

    Ok(())
}

/// Navigate through nested YAML structure using dot notation path
///
/// This function traverses a YAML value structure following a dot-separated path
/// (e.g., "vm.memory" -> value["vm"]["memory"])). Each part of the path must exist
/// as a key in a YAML mapping, or the function returns an error.
///
/// # Arguments
/// * `value` - The root YAML value to traverse
/// * `field` - Dot-separated field path (e.g., "services.postgresql.port")
///
/// # Returns
/// A reference to the value at the specified path, or an error if:
/// - Any part of the path doesn't exist
/// - Any intermediate value is not a mapping/object
fn get_nested_field<'a>(value: &'a Value, field: &str) -> Result<&'a Value> {
    let mut current = value;

    for part in field.split('.') {
        match current {
            Value::Mapping(map) => {
                let key = Value::String(part.into());
                current = map
                    .get(&key)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", part))?;
            }
            _ => {
                vm_error!("Cannot navigate field '{}' on non-object", part);
                return Err(anyhow::anyhow!("Cannot navigate field on non-object"));
            }
        }
    }

    Ok(current)
}

fn unset_nested_field(value: &mut Value, field: &str) -> Result<()> {
    if field.is_empty() {
        bail!("Empty field path");
    }

    let parts: Vec<&str> = field.split('.').collect();
    unset_nested_field_recursive(value, &parts)
}

fn unset_nested_field_recursive(value: &mut Value, parts: &[&str]) -> Result<()> {
    if parts.len() == 1 {
        match value {
            Value::Mapping(map) => {
                let key = Value::String(parts[0].into());
                if map.remove(&key).is_none() {
                    bail!("Field '{}' not found", parts[0]);
                }
                return Ok(());
            }
            _ => {
                bail!("Cannot unset field on non-object");
            }
        }
    }

    match value {
        Value::Mapping(map) => {
            let key = Value::String(parts[0].into());
            match map.get_mut(&key) {
                Some(nested) => unset_nested_field_recursive(nested, &parts[1..])?,
                None => {
                    bail!("Field '{}' not found", parts[0]);
                }
            }
        }
        _ => {
            bail!("Cannot navigate field '{}' on non-object", parts[0]);
        }
    }

    Ok(())
}

/// Load global configuration if it exists
pub fn load_global_config() -> Option<VmConfig> {
    let global_path = get_global_config_path();
    if !global_path.exists() {
        return None;
    }

    VmConfig::from_file(&global_path).ok()
}
