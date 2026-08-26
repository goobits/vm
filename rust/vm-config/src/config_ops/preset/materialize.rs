use std::path::Path;

use vm_core::error::{Result, VmError};

use crate::config::VmConfig;
use crate::config_ops::port_placeholders::load_preset_with_placeholders;
use crate::merge::ConfigMerger;
use crate::preset::PresetDetector;

pub(crate) fn resolve_declared_presets(config: VmConfig, project_dir: &Path) -> Result<VmConfig> {
    let Some(preset_names) = config.preset.clone() else {
        return Ok(config);
    };

    let detector = PresetDetector::new(project_dir.to_path_buf());
    let port_range = config
        .ports
        .range
        .as_ref()
        .and_then(|range| (range.len() == 2).then(|| format!("{}-{}", range[0], range[1])));
    let mut resolved = VmConfig::default();

    for name in preset_names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let preset = load_preset_with_placeholders(&detector, name, &port_range)
            .map_err(|error| VmError::Config(format!("Failed to load preset '{name}': {error}")))?;
        resolved = ConfigMerger::new(resolved).merge(preset)?;
    }

    ConfigMerger::new(resolved).merge(config)
}

pub(super) fn materialize_minimal_preset_config(
    merged: &VmConfig,
    preset_names: &str,
    preset: Option<&VmConfig>,
    original_base_config: Option<&VmConfig>,
    suppress_warning: bool,
) -> (VmConfig, bool) {
    // Build the VM section with the preset image, when specified.
    let vm = build_vm_section(merged, preset);

    // Start with minimal config
    let mut minimal = VmConfig {
        preset: Some(preset_names.to_string()),
        version: merged.version.clone(),
        provider: merged.provider.clone(),
        default_profile: merged.default_profile.clone(),
        tart: merged.tart.clone(),
        project: merged.project.clone(),
        vm,
        ports: merged.ports.clone(),
        services: merged.services.clone(),
        terminal: merged.terminal.clone(),
        profiles: merged.profiles.clone(),
        apt_packages: merged.apt_packages.clone(),
        npm_packages: merged.npm_packages.clone(),
        pip_packages: merged.pip_packages.clone(),
        cargo_packages: merged.cargo_packages.clone(),
        ..Default::default()
    };

    let mut warn_preserved_customizations = false;
    if let Some(original) = original_base_config {
        let has_customizations = preserve_user_customizations(&mut minimal, merged, original);
        warn_preserved_customizations =
            has_customizations && !suppress_warning && original.preset.is_none();
    }

    (minimal, warn_preserved_customizations)
}

/// Build the VM section with the preset image, when specified.
fn build_vm_section(
    merged: &VmConfig,
    preset: Option<&VmConfig>,
) -> Option<crate::config::VmSettings> {
    use crate::config::VmSettings;

    let mut vm = merged.vm.clone();

    // A preset image overrides the merged image.
    if let Some(preset_image) = preset
        .and_then(|p| p.vm.as_ref())
        .and_then(|vm| vm.image.clone())
    {
        if vm.is_none() {
            vm = Some(VmSettings::default());
        }
        if let Some(vm_settings) = vm.as_mut() {
            vm_settings.image = Some(preset_image);
        }
    }

    vm
}

/// Preserve user customizations from original config
fn preserve_user_customizations(
    minimal: &mut VmConfig,
    merged: &VmConfig,
    original: &VmConfig,
) -> bool {
    let mut has_customizations = false;

    if original.versions.is_some() {
        minimal.versions = merged.versions.clone();
        has_customizations = true;
    }

    if !original.storage.is_empty() {
        minimal.storage = merged.storage.clone();
        has_customizations = true;
    }

    if !original.mounts.is_empty() {
        minimal.mounts = merged.mounts.clone();
        has_customizations = true;
    }

    if !crate::config::ToolsConfig::is_empty(&original.tools) {
        minimal.tools = merged.tools.clone();
        has_customizations = true;
    }

    if original.bootstrap.is_some() {
        minimal.bootstrap = merged.bootstrap.clone();
        has_customizations = true;
    }

    if !original.apt_packages.is_empty() {
        minimal.apt_packages = merged.apt_packages.clone();
        has_customizations = true;
    }

    if !original.npm_packages.is_empty() {
        minimal.npm_packages = merged.npm_packages.clone();
        has_customizations = true;
    }

    if !original.pip_packages.is_empty() {
        minimal.pip_packages = merged.pip_packages.clone();
        has_customizations = true;
    }

    if !original.cargo_packages.is_empty() {
        minimal.cargo_packages = merged.cargo_packages.clone();
        has_customizations = true;
    }

    if !original.aliases.is_empty() {
        minimal.aliases = merged.aliases.clone();
        has_customizations = true;
    }

    if !original.environment.is_empty() {
        minimal.environment = merged.environment.clone();
        has_customizations = true;
    }

    if original.host_sync.is_some() {
        minimal.host_sync = merged.host_sync.clone();
        has_customizations = true;
    }

    if original.os.is_some() {
        minimal.os = merged.os.clone();
        has_customizations = true;
    }

    if original.networking.is_some() {
        minimal.networking = merged.networking.clone();
        has_customizations = true;
    }

    if original.default_profile.is_some() {
        minimal.default_profile = merged.default_profile.clone();
        has_customizations = true;
    }

    if original.profiles.is_some() {
        minimal.profiles = merged.profiles.clone();
        has_customizations = true;
    }

    if original.tart.is_some() {
        minimal.tart = merged.tart.clone();
        has_customizations = true;
    }

    has_customizations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_rewrite_preserves_storage_bootstrap_mounts_and_tools() {
        let original: VmConfig = serde_yaml_ng::from_str(
            r#"
storage:
  volumes:
    node_modules:
      target: /workspace/node_modules
bootstrap:
  playwright:
    browsers: [chromium]
mounts:
  - source: ../shared
    target: /shared
    access: read_only
tools:
  codex: {}
"#,
        )
        .unwrap();
        let merged = original.clone();
        let mut minimal = VmConfig::default();

        assert!(preserve_user_customizations(
            &mut minimal,
            &merged,
            &original
        ));
        assert!(minimal.storage.volumes.contains_key("node_modules"));
        assert_eq!(minimal.bootstrap.unwrap().playwright.browsers, ["chromium"]);
        assert_eq!(minimal.mounts.len(), 1);
        assert!(minimal.tools.entries.contains_key("codex"));
    }
}
