// Standard library
use std::fs;

// External crates
use serde_yaml_ng as serde_yaml;
use serde_yaml_ng::Value;

// Internal imports
use crate::config_ops::io::{find_local_config, get_global_config_path};
use vm_core::error::Result;
use vm_core::{vm_error, vm_error_hint, vm_println};
use vm_messages::messages::MESSAGES;

/// Get a configuration value or display entire configuration.
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
            let value = get_nested_field(&yaml_value, f)?;
            match value {
                Value::String(s) => vm_println!("{}", s),
                _ => vm_println!("{}", serde_yaml::to_string(value)?),
            }
        }
        None => {
            vm_println!("{}", MESSAGES.config_current_configuration);
            vm_println!("{}", serde_yaml::to_string(&yaml_value)?);
            vm_println!("{}", MESSAGES.config_modify_hint);
        }
    }

    Ok(())
}

/// Navigate through nested YAML structure using dot notation path
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