// Standard library
use std::path::PathBuf;

// External crates
use serde_yaml_ng as serde_yaml;
use serde_yaml_ng::{Mapping, Value};

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::{
    find_or_create_local_config, get_global_config_path, get_or_create_global_config_path,
    read_config_or_init,
};
use crate::config_ops::plan::{read_document, ConfigEditPlan, ConfigMutationReport};
use crate::schema;
use vm_core::error::Result;
use vm_core::msg;
use vm_core::{vm_println, vm_success};
use vm_messages::messages::MESSAGES;

/// Set a configuration value using dot notation with schema-aware type detection.
pub fn set(
    field: &str,
    values: &[String],
    global: bool,
    dry_run: bool,
    path: Option<PathBuf>,
    value_json: Option<&str>,
    structured: bool,
) -> Result<ConfigMutationReport> {
    let config_path = if global {
        get_global_config_path()
    } else {
        path.map(Ok).unwrap_or_else(find_or_create_local_config)?
    };

    if !global && !dry_run {
        let _ = read_config_or_init(&config_path, true)?;
    }

    if dry_run && !global && !config_path.is_file() {
        return Err(vm_core::error::VmError::Config(format!(
            "Cannot preview a missing project configuration: {}. Run `vm init` first",
            config_path.display()
        )));
    }

    let mut yaml_value = if config_path.exists() {
        read_document(&config_path)?
    } else {
        Value::Mapping(Mapping::new())
    };
    let before = yaml_value.clone();

    // Use schema-aware parsing to handle arrays and other types correctly
    let parsed_value = if let Some(json) = value_json {
        let expected = schema::lookup_field_type(field, global);
        let parsed: serde_json::Value = serde_json::from_str(json).map_err(|error| {
            vm_core::error::VmError::Config(format!("Invalid JSON value for '{field}': {error}"))
        })?;
        let correct_shape = match expected {
            schema::SchemaType::Array { .. } => parsed.is_array(),
            schema::SchemaType::Object => parsed.is_object(),
            _ => false,
        };
        if !correct_shape {
            return Err(vm_core::error::VmError::Config(format!(
                "--value-json requires an array or object configuration field: {field}"
            )));
        }
        serde_yaml::to_value(parsed)?
    } else {
        schema::parse_value_with_schema(field, values, global)?
    };

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

    if !global && field == "default_profile" {
        validate_default_profile(&yaml_value, values.first().map(String::as_str))?;
    }

    super::validate::candidate(&yaml_value, &config_path, global)?;
    let plan = ConfigEditPlan::new(config_path.clone(), before, yaml_value, global);

    // Format the value for display (show as array if multiple values)
    let sensitive_field = [
        "token",
        "password",
        "secret",
        "credential",
        "api_key",
        "private_key",
    ]
    .iter()
    .any(|part| field.to_ascii_lowercase().contains(part));
    let value_display = if sensitive_field {
        "[redacted]".to_string()
    } else if value_json.is_some() {
        "[JSON value]".to_string()
    } else if values.len() == 1 {
        values[0].clone()
    } else {
        format!("[{}]", values.join(", "))
    };

    if dry_run {
        if !structured {
            plan.preview();
        }
    } else {
        if global {
            let _ = get_or_create_global_config_path()?;
        }
        plan.write()?;
        if !structured {
            vm_success!(
                "{}",
                msg!(
                    MESSAGES.config.set_success,
                    field = field,
                    value = value_display,
                    path = config_path.display().to_string()
                )
            );
            vm_println!("{}", MESSAGES.config.apply_changes_hint);
        }
    }
    Ok(plan.report(dry_run, None))
}

fn validate_default_profile(value: &Value, profile_name: Option<&str>) -> Result<()> {
    let Some(profile_name) = profile_name else {
        return Ok(());
    };

    let has_profile = value
        .get("profiles")
        .and_then(Value::as_mapping)
        .is_some_and(|profiles| profiles.contains_key(Value::String(profile_name.to_string())));

    if has_profile {
        return Ok(());
    }

    Err(vm_core::error::VmError::Config(format!(
        "Profile '{profile_name}' not found in vm.yaml. Apply a preset that defines it first, for example: vm config presets apply vibe-tart"
    )))
}

fn set_nested_field(value: &mut Value, field: &str, new_value: Value) -> Result<()> {
    if field.is_empty() {
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
            return Err(vm_core::error::VmError::Config(format!(
                "Cannot navigate field '{}' on non-object value",
                parts[0]
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::ConfigOps;

    #[test]
    fn json_array_replaces_the_field_without_nesting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        std::fs::write(&path, "project:\n  name: test\nprovider: docker\n").unwrap();

        ConfigOps::set_json_at(
            "networking.networks",
            "[\"dev\", \"test\"]",
            false,
            Some(path.clone()),
        )
        .unwrap();

        let contents = std::fs::read_to_string(path).unwrap();
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&contents).unwrap();
        assert_eq!(value["networking"]["networks"][0], "dev");
        assert_eq!(value["networking"]["networks"][1], "test");
    }

    #[test]
    fn json_is_rejected_for_scalar_fields_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let original = "project:\n  name: test\nprovider: docker\n";
        std::fs::write(&path, original).unwrap();

        assert!(
            ConfigOps::set_json_at("provider", "[\"docker\"]", false, Some(path.clone())).is_err()
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn invalid_result_does_not_replace_project_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let original = "project:\n  name: test\nprovider: docker\n";
        std::fs::write(&path, original).unwrap();

        let result = ConfigOps::set_at(
            "services.postgresql.enabled",
            &["true".to_string()],
            false,
            false,
            Some(path.clone()),
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no port specified"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn misspelled_nested_field_is_rejected_before_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let original = "project:\n  name: test\nprovider: docker\n";
        std::fs::write(&path, original).unwrap();

        let result = ConfigOps::set_at(
            "vm.memroy",
            &["4096".to_string()],
            false,
            false,
            Some(path.clone()),
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown configuration field: vm.memroy"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn dry_run_validates_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let original = "project:\n  name: test\nprovider: docker\n";
        std::fs::write(&path, original).unwrap();

        ConfigOps::set_at(
            "vm.memory",
            &["4096".to_string()],
            false,
            true,
            Some(path.clone()),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

        let error = ConfigOps::set_at(
            "vm.memroy",
            &["4096".to_string()],
            false,
            true,
            Some(path.clone()),
        )
        .unwrap_err();
        assert!(error.to_string().contains("Unknown configuration field"));
        ConfigOps::set_json_preview_at(
            "networking.networks",
            "[\"dev\"]",
            false,
            true,
            Some(path.clone()),
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }
}
