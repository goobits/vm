// Standard library
use std::fs;
use std::path::PathBuf;

// External crates
use anyhow::{bail, Context, Result};
use serde_yaml_ng as serde_yaml;
use serde_yaml::{Mapping, Value};

// Internal imports
use crate::config::VmConfig;
use crate::merge::ConfigMerger;
use crate::paths;
use crate::preset::PresetDetector;

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
/// ```rust
/// use vm_config::ConfigOps;
///
/// // Set a local configuration value
/// ConfigOps::set("vm.memory", "4096", false)?;
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
    /// ```rust
    /// use vm_config::ConfigOps;
    ///
    /// // Set VM memory
    /// ConfigOps::set("vm.memory", "4096", false)?;
    ///
    /// // Enable a service globally
    /// ConfigOps::set("services.postgresql.enabled", "true", true)?;
    ///
    /// // Set an array value
    /// ConfigOps::set("npm_packages", "[eslint, prettier]", false)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn set(field: &str, value: &str, global: bool) -> Result<()> {
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

        let yaml_str = serde_yaml::to_string(&yaml_value)?;
        fs::write(&config_path, yaml_str)?;

        println!("✅ Set {} = {} in {}", field, value, config_path.display());
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
    /// ```rust
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
                bail!("No global configuration found at {}", config_path.display());
            } else {
                bail!("No vm.yaml found in current directory or parent directories");
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
            bail!("Configuration file not found: {}", config_path.display());
        }

        let content = fs::read_to_string(&config_path)?;
        let mut yaml_value: Value = serde_yaml::from_str(&content)?;

        unset_nested_field(&mut yaml_value, field)?;

        let yaml_str = serde_yaml::to_string(&yaml_value)?;
        fs::write(&config_path, yaml_str)?;

        println!("✅ Unset {} in {}", field, config_path.display());
        Ok(())
    }

    /// Clear all configuration
    pub fn clear(global: bool) -> Result<()> {
        let config_path = if global {
            get_global_config_path()
        } else {
            find_local_config()?
        };

        if !config_path.exists() {
            println!("No configuration file to clear");
            return Ok(());
        }

        fs::remove_file(&config_path)?;
        println!("✅ Cleared {}", config_path.display());
        Ok(())
    }

    /// Apply preset(s) to configuration
    pub fn preset(preset_names: &str, global: bool, list: bool, show: Option<&str>) -> Result<()> {
        let presets_dir = paths::get_presets_dir();
        let project_dir = std::env::current_dir()?;
        let detector = PresetDetector::new(project_dir, presets_dir.clone());

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
            let preset_path = presets_dir.join(format!("{}.yaml", preset_name));
            if !preset_path.exists() {
                bail!("Preset '{}' not found", preset_name);
            }

            let preset_config = VmConfig::from_file(&preset_path)
                .with_context(|| format!("Failed to load preset: {}", preset_name))?;

            merged_config = ConfigMerger::new(merged_config).merge(preset_config)?;
        }

        // Save merged configuration
        let yaml_str = serde_yaml::to_string(&merged_config)?;
        fs::write(&config_path, yaml_str)?;

        let scope = if global { "global" } else { "local" };
        println!(
            "✅ Applied preset(s) {} to {} configuration",
            preset_names, scope
        );
        Ok(())
    }
}

// Helper functions

fn get_global_config_path() -> PathBuf {
    // Check for test environment override first
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".config")
            .join("vm")
            .join("global.yaml");
    }

    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("vm")
        .join("global.yaml")
}

fn get_or_create_global_config_path() -> Result<PathBuf> {
    let config_dir = if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config").join("vm")
    } else {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("vm")
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

    bail!("No vm.yaml found in current directory or parent directories")
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
    let parts: Vec<&str> = field.split('.').collect();
    if parts.is_empty() {
        bail!("Empty field path");
    }

    set_nested_field_recursive(value, &parts, new_value)
}

fn set_nested_field_recursive(value: &mut Value, parts: &[&str], new_value: Value) -> Result<()> {
    if parts.len() == 1 {
        match value {
            Value::Mapping(map) => {
                let key = Value::String(parts[0].to_string());
                map.insert(key, new_value);
                return Ok(());
            }
            _ => bail!("Cannot set field on non-object"),
        }
    }

    match value {
        Value::Mapping(map) => {
            let key = Value::String(parts[0].to_string());
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
        _ => bail!("Cannot navigate field '{}' on non-object", parts[0]),
    }

    Ok(())
}

fn get_nested_field<'a>(value: &'a Value, field: &str) -> Result<&'a Value> {
    let mut current = value;

    for part in field.split('.') {
        match current {
            Value::Mapping(map) => {
                let key = Value::String(part.to_string());
                current = map
                    .get(&key)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found", part))?;
            }
            _ => bail!("Cannot navigate field '{}' on non-object", part),
        }
    }

    Ok(current)
}

fn unset_nested_field(value: &mut Value, field: &str) -> Result<()> {
    let parts: Vec<&str> = field.split('.').collect();
    if parts.is_empty() {
        bail!("Empty field path");
    }

    unset_nested_field_recursive(value, &parts)
}

fn unset_nested_field_recursive(value: &mut Value, parts: &[&str]) -> Result<()> {
    if parts.len() == 1 {
        match value {
            Value::Mapping(map) => {
                let key = Value::String(parts[0].to_string());
                if map.remove(&key).is_none() {
                    bail!("Field '{}' not found", parts[0]);
                }
                return Ok(());
            }
            _ => bail!("Cannot unset field on non-object"),
        }
    }

    match value {
        Value::Mapping(map) => {
            let key = Value::String(parts[0].to_string());
            match map.get_mut(&key) {
                Some(nested) => unset_nested_field_recursive(nested, &parts[1..])?,
                None => bail!("Field '{}' not found", parts[0]),
            }
        }
        _ => bail!("Cannot navigate field '{}' on non-object", parts[0]),
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
