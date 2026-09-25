use crate::cli::PluginSubcommand;
use crate::error::{VmError, VmResult};

mod install;
mod inventory;
mod validation;
mod view;

use install::{handle_plugin_install, handle_plugin_remove};
use inventory::{handle_plugin_info, handle_plugin_list, handle_plugin_validate};

pub(super) fn handle_command(command: &PluginSubcommand) -> VmResult<()> {
    match command {
        PluginSubcommand::List { json } => handle_plugin_list(*json).map_err(VmError::from),
        PluginSubcommand::Show { plugin_name, json } => handle_plugin_info(plugin_name, *json)
            .map_err(|error| VmError::from(error).with_target(plugin_name)),
        PluginSubcommand::Install { source_path } => {
            handle_plugin_install(source_path).map_err(VmError::from)
        }
        PluginSubcommand::Remove { plugin_name } => {
            handle_plugin_remove(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::Create { plugin_name, kind } => {
            super::plugin_new::handle_plugin_new(plugin_name, kind).map_err(VmError::from)
        }
        PluginSubcommand::Validate { plugin_name } => {
            handle_plugin_validate(plugin_name).map_err(VmError::from)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::handle_plugin_validate;
    use super::install::install_validated_plugin;
    use super::validation::{
        plugin_from_source, validate_all, validate_plugin_files, validate_plugin_source,
    };
    use std::fs;
    use vm_plugin::{PluginInfo, PluginType};

    fn preset_info() -> PluginInfo {
        PluginInfo {
            name: "unique-test-preset-plugin".to_string(),
            version: "1.0.0".to_string(),
            description: Some("Test preset".to_string()),
            author: None,
            plugin_type: PluginType::Preset,
            preset_category: None,
        }
    }

    #[test]
    fn plugin_install_stages_only_declared_files() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        let info = preset_info();
        fs::write(
            source.join("plugin.yaml"),
            serde_yaml_ng::to_string(&info).unwrap(),
        )
        .unwrap();
        fs::write(source.join("preset.yaml"), "packages: [git]\n").unwrap();
        fs::write(source.join("README.md"), "Documentation\n").unwrap();
        handle_plugin_validate(source.to_str().unwrap()).unwrap();
        let plugins = root.path().join("state").join("plugins");
        install_validated_plugin(&source, &plugins, "presets", &info, "preset.yaml").unwrap();
        let target = plugins.join("presets").join(&info.name);
        assert!(target.join("plugin.yaml").is_file());
        assert!(target.join("preset.yaml").is_file());
        assert!(target.join("README.md").is_file());
        assert!(
            install_validated_plugin(&source, &plugins, "presets", &info, "preset.yaml").is_err()
        );
        fs::write(source.join("run.sh"), "echo hi\n").unwrap();
        assert!(validate_plugin_files(&source, "preset.yaml").is_err());
    }

    #[test]
    fn plugin_validation_rejects_nested_settings_that_cannot_apply() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("plugin.yaml"),
            serde_yaml_ng::to_string(&preset_info()).unwrap(),
        )
        .unwrap();
        fs::write(source.join("preset.yaml"), "mounts: invalid\n").unwrap();
        let plugin = plugin_from_source(&source).unwrap();
        assert!(!validate_all(&plugin).unwrap().is_valid);
        assert!(install_validated_plugin(
            &source,
            &root.path().join("state/plugins"),
            "presets",
            &preset_info(),
            "preset.yaml",
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn plugin_install_rejects_links_and_keeps_target_absent() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(
            source.join("plugin.yaml"),
            serde_yaml_ng::to_string(&preset_info()).unwrap(),
        )
        .unwrap();
        fs::write(source.join("preset.yaml"), "packages: [git]\n").unwrap();
        symlink(source.join("preset.yaml"), source.join("linked.yaml")).unwrap();
        assert!(validate_plugin_files(&source, "preset.yaml").is_err());
        assert!(validate_plugin_source(&source.join("linked.yaml")).is_err());
        assert!(!root
            .path()
            .join("state/plugins/presets/unique-test-preset-plugin")
            .exists());
    }
}
