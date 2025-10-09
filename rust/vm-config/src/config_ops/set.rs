// Standard library
use std::fs;

// External crates
use serde_yaml_ng as serde_yaml;
use serde_yaml_ng::{Mapping, Value};

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::{
    find_or_create_local_config, get_or_create_global_config_path, read_config_or_init,
};
use crate::yaml::core::CoreOperations;
use vm_cli::msg;
use vm_core::error::Result;
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;

/// Set a configuration value using dot notation.
pub fn set(field: &str, value: &str, global: bool, dry_run: bool) -> Result<()> {
    let config_path = if global {
        get_or_create_global_config_path()?
    } else {
        find_or_create_local_config()?
    };

    if !global {
        let _ = read_config_or_init(&config_path, true)?;
    }

    let mut yaml_value = if config_path.exists() {
        let content = fs::read_to_string(&config_path)?;
        serde_yaml::from_str(&content)?
    } else {
        Value::Mapping(Mapping::new())
    };

    let parsed_value: Value =
        serde_yaml::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));

    set_nested_field(&mut yaml_value, field, parsed_value)?;

    let should_allocate_ports = field.starts_with("services.") && field.ends_with(".enabled");

    if should_allocate_ports {
        let has_port_range = yaml_value
            .get("ports")
            .and_then(|p| p.get("_range"))
            .and_then(|r| r.as_sequence())
            .is_some_and(|seq| seq.len() == 2);

        if has_port_range {
            let mut config: VmConfig = serde_yaml::from_value(yaml_value.clone())?;
            config.ensure_service_ports();
            yaml_value = serde_yaml::to_value(&config)?;
        }
    }

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
