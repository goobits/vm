use tracing::{info, instrument};
use vm_core::error::{Result, VmError};
use vm_plugin::PresetCategory;

use crate::{config::VmConfig, merge::ConfigMerger, paths, preset::PresetDetector};

use super::build_initial_config;

#[instrument(fields(sanitized_name, preset_name))]
pub(super) fn build_config_from_preset(
    sanitized_name: &str,
    preset_name: &str,
) -> Result<VmConfig> {
    let detector = PresetDetector::new(std::env::current_dir()?, paths::get_presets_dir());
    let available = detector.list_all_presets()?;
    if !available.iter().any(|name| name == preset_name) {
        return Err(VmError::Config(format!(
            "Preset '{preset_name}' not found. Available presets: {}",
            available.join(", ")
        )));
    }

    let preset = detector.load_preset_cached(preset_name)?;
    let category = preset_category(preset_name, &preset);
    let base = build_initial_config(sanitized_name)?;
    let mut config = match category {
        PresetCategory::Box => {
            info!("🎁 Using box preset '{preset_name}'");
            apply_box_preset(base, preset)
        }
        PresetCategory::Provision => {
            info!("📦 Using provision preset '{preset_name}'");
            ConfigMerger::new(base).merge(preset)?
        }
    };
    config.preset = Some(preset_name.to_string());
    Ok(config)
}

fn preset_category(preset_name: &str, preset: &VmConfig) -> PresetCategory {
    if let Ok(plugins) = vm_plugin::discover_plugins() {
        if let Some(plugin) = plugins.into_iter().find(|plugin| {
            plugin.info.plugin_type == vm_plugin::PluginType::Preset
                && plugin.info.name == preset_name
        }) {
            if let Some(category) = plugin.info.preset_category {
                return category;
            }
            if let Ok(content) = vm_plugin::load_preset_content(&plugin) {
                return content.category;
            }
        }
    }

    if preset
        .vm
        .as_ref()
        .and_then(|vm| vm.r#box.as_ref())
        .is_some()
    {
        PresetCategory::Box
    } else {
        PresetCategory::Provision
    }
}

fn apply_box_preset(mut config: VmConfig, mut preset: VmConfig) -> VmConfig {
    replace_if_some(&mut config.provider, preset.provider.take());
    replace_if_some(&mut config.default_profile, preset.default_profile.take());
    replace_if_some(&mut config.os, preset.os.take());
    replace_if_some(&mut config.tart, preset.tart.take());
    replace_if_some(&mut config.profiles, preset.profiles.take());

    if let Some(box_spec) = preset.vm.and_then(|vm| vm.r#box) {
        config.vm.get_or_insert_with(Default::default).r#box = Some(box_spec);
    }
    if let Some(mut source) = preset.project {
        let target = config.project.get_or_insert_with(Default::default);
        replace_if_some(&mut target.workspace_path, source.workspace_path.take());
        replace_if_some(&mut target.backup_pattern, source.backup_pattern.take());
        replace_if_some(
            &mut target.env_template_path,
            source.env_template_path.take(),
        );
    }
    if preset.ports.has_ports() {
        config.ports = preset.ports;
    }
    replace_if_some(&mut config.networking, preset.networking);
    if !preset.mounts.is_empty() {
        config.mounts = preset.mounts;
    }
    if !crate::config::ToolsConfig::is_empty(&preset.tools) {
        config.tools = preset.tools;
    }
    if !preset.aliases.is_empty() {
        config.aliases = preset.aliases;
    }
    if !preset.environment.is_empty() {
        config.environment = preset.environment;
    }
    replace_if_some(&mut config.terminal, preset.terminal);
    replace_if_some(&mut config.host_sync, preset.host_sync);
    replace_if_some(&mut config.security, preset.security);

    config.versions = None;
    config.apt_packages.clear();
    config.npm_packages.clear();
    config.pip_packages.clear();
    config.cargo_packages.clear();
    config
}

fn replace_if_some<T>(target: &mut Option<T>, source: Option<T>) {
    if source.is_some() {
        *target = source;
    }
}
