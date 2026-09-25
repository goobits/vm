// Configuration command dispatch.

use crate::cli::{
    ConfigPresetSubcommand, ConfigProfileSubcommand, ConfigSubcommand, ConfigWriteScope,
};
use crate::error::{VmError, VmResult};
use std::path::PathBuf;
use vm_config::ConfigOps;

mod ports;
mod read;

use ports::handle_ports_command;
use read::{
    find_project_config, handle_get_command, handle_profile_list, handle_profile_set,
    handle_profile_show, handle_render_command, handle_show_command, handle_validate_command,
    report_unset_effective,
};

/// Handle configuration management commands
pub fn handle_config_command(
    command: &ConfigSubcommand,
    profile: Option<String>,
    config_path: Option<PathBuf>,
) -> VmResult<()> {
    match command {
        ConfigSubcommand::Validate => handle_validate_command(config_path, profile),
        ConfigSubcommand::Show { scope, json } => {
            handle_show_command(config_path, profile, *scope, *json)
        }
        ConfigSubcommand::Render { env } => {
            handle_render_command(config_path, profile, env.as_deref())
        }
        ConfigSubcommand::Set {
            field,
            values,
            value_json,
            scope,
        } => {
            let path = project_write_path(*scope, config_path)?;
            if let Some(json) = value_json {
                Ok(ConfigOps::set_json_at(
                    field,
                    json,
                    *scope == ConfigWriteScope::User,
                    path,
                )?)
            } else {
                Ok(ConfigOps::set_at(
                    field,
                    values,
                    *scope == ConfigWriteScope::User,
                    false,
                    path,
                )?)
            }
        }
        ConfigSubcommand::Get { field, scope, json } => {
            handle_get_command(field, *scope, config_path, profile, *json)
        }
        ConfigSubcommand::Unset { field, scope } => {
            let path = project_write_path(*scope, config_path.clone())?;
            ConfigOps::unset_at(field, *scope == ConfigWriteScope::User, path)?;
            report_unset_effective(field, *scope, config_path, profile);
            Ok(())
        }
        ConfigSubcommand::Presets { command } => match command {
            ConfigPresetSubcommand::List => {
                Ok(ConfigOps::preset_at("", false, true, None, config_path)?)
            }
            ConfigPresetSubcommand::Show { name } => Ok(ConfigOps::preset_at(
                "",
                false,
                false,
                Some(name),
                config_path,
            )?),
            ConfigPresetSubcommand::Apply { names, scope } => {
                let path = project_write_path(*scope, config_path)?;
                Ok(ConfigOps::preset_at(
                    &names.join(","),
                    *scope == ConfigWriteScope::User,
                    false,
                    None,
                    path,
                )?)
            }
        },
        ConfigSubcommand::Profiles { command } => match command {
            ConfigProfileSubcommand::List => handle_profile_list(config_path),
            ConfigProfileSubcommand::Show { name } => handle_profile_show(name, config_path),
            ConfigProfileSubcommand::SetDefault { name } => {
                let path = project_write_path(ConfigWriteScope::Project, config_path)?;
                handle_profile_set(name, path)
            }
        },
        ConfigSubcommand::Ports { fix } => handle_ports_command(
            *fix,
            project_write_path(ConfigWriteScope::Project, config_path)?,
        ),
    }
}

fn project_write_path(
    scope: ConfigWriteScope,
    config_path: Option<PathBuf>,
) -> VmResult<Option<PathBuf>> {
    if scope == ConfigWriteScope::Project {
        let path = config_path.map(Ok).unwrap_or_else(find_project_config)?;
        if !path.is_file() {
            return Err(VmError::validation(
                format!("Project configuration does not exist: {}", path.display()),
                None::<String>,
            ));
        }
        return Ok(Some(path));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::read::{load_selected_config, redact_yaml};
    use super::{handle_config_command, handle_validate_command};
    use crate::cli::{ConfigSubcommand, ConfigWriteScope};

    #[test]
    fn validation_honors_explicit_config_and_does_not_modify_it() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        let contents = b"project:\n  name: selected\nprovider: docker\n";
        std::fs::write(&config_path, contents).unwrap();

        handle_validate_command(Some(config_path.clone()), None).unwrap();

        assert_eq!(std::fs::read(config_path).unwrap(), contents);
    }

    #[test]
    fn validation_rejects_misspelled_nested_fields() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("vm.yaml");
        let contents = "project:\n  name: selected\nprovider: docker\nvm:\n  memroy: 4096\n";
        std::fs::write(&config_path, contents).unwrap();
        let error = handle_validate_command(Some(config_path.clone()), None).unwrap_err();
        assert!(error
            .to_string()
            .contains("Unknown configuration field: vm.memroy"));
        assert_eq!(std::fs::read_to_string(config_path).unwrap(), contents);
    }

    #[test]
    fn selected_profile_is_loaded_from_explicit_config() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        std::fs::write(
            &config_path,
            r#"
project:
  name: base
provider: docker
profiles:
  feature:
    project:
      name: feature
"#,
        )
        .unwrap();

        let loaded = load_selected_config(Some(config_path), Some("feature".to_string())).unwrap();
        assert_eq!(
            loaded
                .vm
                .project
                .and_then(|project| project.name)
                .as_deref(),
            Some("feature")
        );
    }

    #[test]
    fn redaction_covers_nested_sensitive_fields() {
        let mut value: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            "packages:\n  auth_token: private\nitems:\n  - credentials:\n      password: hidden\nname: visible\n",
        )
        .unwrap();
        redact_yaml(&mut value, "");
        let output = serde_yaml_ng::to_string(&value).unwrap();
        assert!(!output.contains("private"));
        assert!(!output.contains("hidden"));
        assert!(output.contains("visible"));
    }

    #[test]
    fn project_write_uses_explicit_config_path() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        std::fs::write(
            &config_path,
            "project:\n  name: selected\nprovider: docker\n",
        )
        .unwrap();
        handle_config_command(
            &ConfigSubcommand::Set {
                field: "vm.memory".to_string(),
                values: vec!["4096".to_string()],
                value_json: None,
                scope: ConfigWriteScope::Project,
            },
            None,
            Some(config_path.clone()),
        )
        .unwrap();
        let content = std::fs::read_to_string(config_path).unwrap();
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&content).unwrap();
        assert_eq!(value["vm"]["memory"].as_str(), Some("4096"));
    }

    #[test]
    fn explicit_project_write_requires_existing_config() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("missing.yaml");
        let result = handle_config_command(
            &ConfigSubcommand::Set {
                field: "vm.memory".to_string(),
                values: vec!["4096".to_string()],
                value_json: None,
                scope: ConfigWriteScope::Project,
            },
            None,
            Some(config_path.clone()),
        );
        assert!(result.is_err());
        assert!(!config_path.exists());
    }
}
