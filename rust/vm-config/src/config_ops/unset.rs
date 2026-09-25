// Standard library
use std::fs;
use std::path::PathBuf;

// External crates
use serde_yaml_ng::Value;

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::{find_local_config, get_global_config_path, read_config_or_init};
use crate::config_ops::plan::{read_document, ConfigEditPlan, ConfigMutationReport};
use crate::config_ops::preset::resolve_declared_presets;
use vm_core::error::Result;
use vm_core::msg;
use vm_core::{vm_println, vm_success};
use vm_messages::messages::MESSAGES;

/// Unset (remove) a configuration field
pub fn unset(
    field: &str,
    global: bool,
    dry_run: bool,
    path: Option<PathBuf>,
    structured: bool,
) -> Result<ConfigMutationReport> {
    let config_path = if global {
        get_global_config_path()
    } else {
        path.map(Ok).unwrap_or_else(find_local_config)?
    };

    if !global && !dry_run {
        let _ = read_config_or_init(&config_path, true)?;
    } else if !config_path.exists() {
        return Err(vm_core::error::VmError::Config(format!(
            "Configuration file not found at '{}'",
            config_path.display()
        )));
    }

    let mut yaml_value = read_document(&config_path)?;
    let before = yaml_value.clone();

    let config: VmConfig = serde_yaml_ng::from_value(yaml_value.clone())?;
    if config.preset.is_some() {
        let project_dir = config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        yaml_value = serde_yaml_ng::to_value(resolve_declared_presets(config, project_dir)?)?;

        // Materialize inherited values before removal so the preset cannot restore
        // the field on the next load.
        unset_nested_field(&mut yaml_value, "preset")?;
    }

    if field != "preset" {
        unset_nested_field(&mut yaml_value, field)?;
    }

    super::validate::candidate(&yaml_value, &config_path, global)?;
    let plan = ConfigEditPlan::new(config_path.clone(), before, yaml_value, global);
    if dry_run {
        if !structured {
            plan.preview();
        }
        return Ok(plan.report(true, None));
    }

    plan.write()?;

    if !structured {
        vm_success!(
            "{}",
            msg!(
                MESSAGES.config.unset_success,
                field = field,
                path = config_path.display().to_string()
            )
        );
    }
    Ok(plan.report(false, None))
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

#[cfg(test)]
mod tests {
    use crate::ConfigOps;

    #[test]
    fn required_field_unset_keeps_original_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let original = "project:\n  name: test\nprovider: docker\n";
        std::fs::write(&path, original).unwrap();

        let result = ConfigOps::unset_at("provider", false, Some(path.clone()));
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing required field: provider"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn dry_run_unset_preserves_file_and_checks_required_fields() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let original = "project:\n  name: test\nprovider: docker\nvm:\n  memory: '4096'\n";
        std::fs::write(&path, original).unwrap();

        ConfigOps::unset_preview_at("vm.memory", false, true, Some(path.clone())).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        assert!(ConfigOps::unset_preview_at("provider", false, true, Some(path.clone())).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }
}
