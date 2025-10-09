// Standard library
use std::fs;

// External crates
use serde_yaml_ng as serde_yaml;
use serde_yaml_ng::Value;

// Internal imports
use crate::config_ops::io::{find_local_config, get_global_config_path, read_config_or_init};
use crate::yaml::core::CoreOperations;
use vm_cli::msg;
use vm_core::error::Result;
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;

/// Unset (remove) a configuration field
pub fn unset(field: &str, global: bool) -> Result<()> {
    let config_path = if global {
        get_global_config_path()
    } else {
        find_local_config()?
    };

    if !global {
        let _ = read_config_or_init(&config_path, true)?;
    } else if !config_path.exists() {
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
        vm_core::error::VmError::Filesystem(format!(
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
