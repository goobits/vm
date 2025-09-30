//! Configuration management operations for VM Tool.
//!
//! This module provides high-level operations for managing VM configuration files,
//! including setting, getting, and unsetting configuration values using dot notation,
//! applying presets, and managing configuration state with dry-run capabilities.
//! It serves as the core configuration manipulation engine for the CLI.

// Standard library
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

// External crates
use regex::Regex;
use serde_yaml::{Mapping, Value};
use serde_yaml_ng as serde_yaml;
use vm_core::error::Result;
use vm_core::error::VmError;

// Internal imports
use crate::config::VmConfig;
use crate::merge::ConfigMerger;
use crate::paths;
use crate::ports::PortRange;
use crate::preset::{PresetDetector, PresetFile};
use crate::yaml::core::CoreOperations;
use vm_cli::msg;
use vm_core::user_paths;
use vm_core::{vm_error, vm_error_hint, vm_println, vm_warning};
use vm_messages::messages::MESSAGES;

static PORT_PLACEHOLDER_RE: OnceLock<Regex> = OnceLock::new();

fn get_port_placeholder_regex() -> &'static Regex {
    PORT_PLACEHOLDER_RE.get_or_init(|| {
        // This regex pattern is known to be valid, but we handle the error case gracefully
        Regex::new(r"\$\{port\.(\d+)\}").unwrap_or_else(|_| {
            // Fallback to a pattern that never matches if the main one fails
            Regex::new(r"never_matches_anything_specific_placeholder_12345").unwrap_or_else(|_| {
                // Final fallback - empty regex is always valid
                Regex::new("").unwrap_or_else(|_| {
                    panic!("Critical: Even empty regex pattern is failing - regex engine corrupted")
                })
            })
        })
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
            serde_yaml::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));

        set_nested_field(&mut yaml_value, field, parsed_value)?;

        if dry_run {
            vm_println!(
                "🔍 DRY RUN - Would set {} = {} in {}",
                field,
                value,
                config_path.display()
            );
            vm_println!("{}", MESSAGES.config_no_changes);
        } else {
            CoreOperations::write_yaml_file(&config_path, &yaml_value)?;
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.config_set_success,
                    field = field,
                    value = value,
                    path = config_path.display().to_string()
                )
            );
            vm_println!("{}", MESSAGES.config_apply_changes_hint);
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
                return Err(vm_core::error::VmError::Config(format!(
                    "No global configuration found at '{}'. Global configuration is created automatically when needed",
                    config_path.display()
                )));
            } else {
                vm_error!("No vm.yaml found in current directory or parent directories");
                vm_error_hint!("Create one with: vm init");
                return Err(vm_core::error::VmError::Config(
                    "No vm.yaml found in current directory or parent directories. Create one with: vm init".to_string()
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
                    Value::String(s) => vm_println!("{}", s),
                    _ => vm_println!("{}", serde_yaml::to_string(value)?),
                }
            }
            None => {
                // Show all configuration with nice formatting
                vm_println!("{}", MESSAGES.config_current_configuration);
                vm_println!("{}", serde_yaml::to_string(&yaml_value)?);
                vm_println!("{}", MESSAGES.config_modify_hint);
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
            return Err(vm_core::error::VmError::Config(format!(
                "Configuration file not found at '{}'. Use 'vm init' to create a configuration",
                config_path.display()
            )));
        }

        let content = fs::read_to_string(&config_path)?;
        let mut yaml_value: Value = serde_yaml::from_str(&content)?;

        unset_nested_field(&mut yaml_value, field)?;

        CoreOperations::write_yaml_file(&config_path, &yaml_value)?;

        vm_println!(
            "{}",
            msg!(
                MESSAGES.config_unset_success,
                field = field,
                path = config_path.display().to_string()
            )
        );
        Ok(())
    }

    /// Clear (delete) configuration file
    ///
    /// Removes the entire configuration file from the filesystem. This is a
    /// destructive operation that cannot be undone.
    ///
    /// # Arguments
    /// * `global` - If true, clears global config; if false, clears local config
    ///
    /// # Returns
    /// `Ok(())` if the file was successfully removed or didn't exist
    ///
    /// # Errors
    /// Returns an error if:
    /// - File system permissions prevent deletion
    /// - Configuration file is locked by another process
    ///
    /// # Examples
    /// ```rust,no_run
    /// use vm_config::ConfigOps;
    ///
    /// // Clear local configuration
    /// ConfigOps::clear(false)?;
    ///
    /// // Clear global configuration
    /// ConfigOps::clear(true)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn clear(global: bool) -> Result<()> {
        let config_path = if global {
            get_global_config_path()
        } else {
            find_local_config()?
        };

        if !config_path.exists() {
            let config_type = if global { "global" } else { "local" };
            vm_println!("No {} configuration file found to clear", config_type);
            return Ok(());
        }

        fs::remove_file(&config_path).map_err(|e| {
            VmError::Filesystem(format!(
                "Failed to remove configuration file: {}: {}",
                config_path.display(),
                e
            ))
        })?;

        let config_type = if global { "global" } else { "local" };
        vm_println!(
            "✅ Cleared {} configuration: {}",
            config_type,
            config_path.display()
        );

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
            vm_println!("{}", MESSAGES.config_available_presets);
            for preset in presets {
                // Get description dynamically from plugin metadata or embedded preset
                let description = detector
                    .get_preset_description(&preset)
                    .map(|d| format!(" - {}", d))
                    .unwrap_or_default();
                vm_println!("  • {}{}", preset, description);
            }
            vm_println!(
                "{}",
                msg!(MESSAGES.config_apply_preset_hint, name = "<name>")
            );
            return Ok(());
        }

        // Handle show command
        if let Some(name) = show {
            let preset_config = detector.load_preset(name)?;
            let yaml = serde_yaml::to_string(&preset_config)?;
            vm_println!("📋 Preset '{}' configuration:\n", name);
            vm_println!("{}", yaml);
            vm_println!("{}", msg!(MESSAGES.config_apply_preset_hint, name = name));
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
            // Convert port range to string for placeholder replacement
            let port_range_str = merged_config.ports.range.as_ref().and_then(|range| {
                if range.len() == 2 {
                    Some(format!("{}-{}", range[0], range[1]))
                } else {
                    None
                }
            });

            // Load preset with optimized placeholder replacement
            let preset_config =
                load_preset_with_placeholders(&detector, preset_name, &port_range_str).map_err(
                    |e| VmError::Config(format!("Failed to load preset: {}: {}", preset_name, e)),
                )?;

            merged_config = ConfigMerger::new(merged_config).merge(preset_config)?;
        }

        // Save merged configuration with consistent formatting
        // Convert VmConfig to Value first
        let config_yaml = serde_yaml::to_string(&merged_config)?;
        let config_value: Value = serde_yaml::from_str(&config_yaml)?;
        CoreOperations::write_yaml_file(&config_path, &config_value)?;

        let scope = if global { "global" } else { "local" };
        vm_println!(
            "{}",
            msg!(
                MESSAGES.config_preset_applied,
                preset = preset_names,
                path = scope
            )
        );

        // Show what was applied for clarity
        let preset_list: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
        if preset_list.len() > 1 {
            vm_println!("{}", MESSAGES.config_applied_presets);
            for preset in preset_list {
                vm_println!("    • {}", preset);
            }
        }

        vm_println!("{}", MESSAGES.config_restart_hint);
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
    let raw_content = {
        // Try plugin presets first
        if let Ok(plugins) = vm_plugin::discover_plugins() {
            let plugin_preset = plugins.iter().find(|p| {
                p.info.plugin_type == vm_plugin::PluginType::Preset && p.info.name == preset_name
            });

            if plugin_preset.is_some() {
                // For plugin presets, use detector.load_preset which handles conversion
                let original_config = detector.load_preset(preset_name)?;
                let Some(port_range_str) = port_range_str else {
                    return Ok(original_config);
                };
                let mut preset_value = serde_yaml::to_value(&original_config)?;
                replace_port_placeholders(&mut preset_value, &Some(port_range_str.clone()));
                return Ok(serde_yaml::from_value(preset_value)?);
            }
        }

        // Try embedded presets second (optimization for base, tart-* presets)
        if let Some(content) = crate::embedded_presets::get_preset_content(preset_name) {
            content.to_string()
        } else {
            // For filesystem presets, we'd need access to presets_dir which is private
            // For now, fall back to the original method and then do the replacement
            let original_config = detector.load_preset(preset_name)?;

            // If no port_range, return the original config
            let Some(port_range_str) = port_range_str else {
                return Ok(original_config);
            };

            // For non-embedded presets, we fall back to the Value-based approach
            let mut preset_value = serde_yaml::to_value(&original_config)?;
            replace_port_placeholders(&mut preset_value, &Some(port_range_str.clone()));
            return Ok(serde_yaml::from_value(preset_value)?);
        }
    };

    // Replace placeholders in the raw string if port_range is available
    let processed_content = if let Some(port_range_str) = port_range_str {
        replace_placeholders_in_string(&raw_content, port_range_str)?
    } else {
        raw_content
    };

    // Parse the processed content directly to VmConfig
    let preset_file: PresetFile = serde_yaml::from_str(&processed_content)
        .map_err(|e| VmError::Config(format!("Failed to parse preset '{}': {}", preset_name, e)))?;

    Ok(preset_file.config)
}

/// Replace placeholders in raw YAML string - much more efficient than serialize/deserialize
fn replace_placeholders_in_string(content: &str, port_range_str: &str) -> Result<String> {
    let port_range = match PortRange::parse(port_range_str) {
        Ok(range) => range,
        Err(_) => {
            vm_warning!("Could not parse port_range '{}'", port_range_str);
            return Ok(content.to_owned());
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
    // Use unified path with automatic migration
    user_paths::global_config_path().unwrap_or_else(|_| {
        // Test environment fallback
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".vm").join("config.yaml")
        } else {
            PathBuf::from(".vm/config.yaml")
        }
    })
}

fn get_or_create_global_config_path() -> Result<PathBuf> {
    // Use the unified path structure
    let config_path = get_global_config_path();

    // Ensure parent directory exists
    if let Some(parent) = config_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                VmError::Filesystem(format!(
                    "Failed to create config directory: {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
    }

    Ok(config_path)
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
    Err(vm_core::error::VmError::Config(
        "No vm.yaml configuration found in current directory or parent directories. Create one with: vm init".to_string()
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
        return Err(vm_core::error::VmError::Config(
            "Empty field path provided. Specify a field name like 'provider' or 'project.name'"
                .to_string(),
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
                return Err(vm_core::error::VmError::Config(format!(
                    "Cannot set field '{}' on non-object value. Field path may be invalid",
                    parts[0]
                )));
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
            return Err(vm_core::error::VmError::Config(
                "Cannot navigate field on non-object".to_string(),
            ));
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
                current = map.get(&key).ok_or_else(|| {
                    vm_core::error::VmError::Config(format!("Field '{}' not found", part))
                })?;
            }
            _ => {
                vm_error!("Cannot navigate field '{}' on non-object", part);
                return Err(vm_core::error::VmError::Config(
                    "Cannot navigate field on non-object".to_string(),
                ));
            }
        }
    }

    Ok(current)
}

fn unset_nested_field(value: &mut Value, field: &str) -> Result<()> {
    if field.is_empty() {
        return Err(vm_core::error::VmError::Config(
            "Empty field path".to_string(),
        ));
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
                    return Err(vm_core::error::VmError::Config(format!(
                        "Field '{}' not found",
                        parts[0]
                    )));
                }
                return Ok(());
            }
            _ => {
                return Err(vm_core::error::VmError::Config(
                    "Cannot unset field on non-object".to_string(),
                ));
            }
        }
    }

    match value {
        Value::Mapping(map) => {
            let key = Value::String(parts[0].into());
            match map.get_mut(&key) {
                Some(nested) => unset_nested_field_recursive(nested, &parts[1..])?,
                None => {
                    return Err(vm_core::error::VmError::Config(format!(
                        "Field '{}' not found",
                        parts[0]
                    )));
                }
            }
        }
        _ => {
            return Err(vm_core::error::VmError::Config(format!(
                "Cannot navigate field '{}' on non-object",
                parts[0]
            )));
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
