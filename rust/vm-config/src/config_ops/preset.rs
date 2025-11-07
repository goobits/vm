// External crates
use serde_yaml_ng as serde_yaml;

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::{
    find_or_create_local_config, get_or_create_global_config_path, read_config_or_init,
};
use crate::config_ops::port_placeholders::load_preset_with_placeholders;
use crate::merge::ConfigMerger;
use crate::paths;
use crate::preset::PresetDetector;
use crate::yaml::core::CoreOperations;
use vm_cli::msg;
use vm_core::error::{Result, VmError};
use vm_core::{vm_println, vm_success};
use vm_messages::messages::MESSAGES;

/// Apply preset(s) to configuration
pub fn preset(preset_names: &str, global: bool, list: bool, show: Option<&str>) -> Result<()> {
    let presets_dir = paths::get_presets_dir();
    let project_dir = std::env::current_dir()?;
    let detector = PresetDetector::new(project_dir, presets_dir);

    if list {
        let presets = detector.list_presets()?;
        vm_println!("{}", MESSAGES.config_available_presets);
        for preset in presets {
            let description = detector
                .get_preset_description(&preset)
                .map(|d| format!(" - {d}"))
                .unwrap_or_default();
            vm_println!("  • {}{}", preset, description);
        }
        vm_println!(
            "{}",
            msg!(MESSAGES.config_apply_preset_hint, name = "<name>")
        );
        return Ok(());
    }

    if let Some(name) = show {
        let preset_config = detector.load_preset(name)?;
        let yaml = serde_yaml::to_string(&preset_config)?;
        vm_println!("📋 Preset '{}' configuration:\n", name);
        vm_println!("{}", yaml);
        vm_println!("{}", msg!(MESSAGES.config_apply_preset_hint, name = name));
        return Ok(());
    }

    let config_path = if global {
        get_or_create_global_config_path()?
    } else {
        find_or_create_local_config()?
    };

    let base_config = if global {
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let source_desc = format!("{}", config_path.display());
            CoreOperations::parse_yaml_with_diagnostics(&content, &source_desc)?
        } else {
            VmConfig::default()
        }
    } else {
        read_config_or_init(&config_path, true)?
    };

    let preset_iter = preset_names.split(',').map(|s| s.trim());
    let mut merged_config = base_config;
    let mut last_preset_config: Option<VmConfig> = None;

    for preset_name in preset_iter {
        let port_range_str = merged_config.ports.range.as_ref().and_then(|range| {
            if range.len() == 2 {
                Some(format!("{}-{}", range[0], range[1]))
            } else {
                None
            }
        });

        let preset_config = load_preset_with_placeholders(&detector, preset_name, &port_range_str)
            .map_err(|e| VmError::Config(format!("Failed to load preset: {preset_name}: {e}")))?;

        merged_config = ConfigMerger::new(merged_config).merge(preset_config.clone())?;
        last_preset_config = Some(preset_config);
    }

    // Create minimal config with only project-specific fields
    // Everything else (packages, versions, aliases, host_sync, etc.) comes from preset/defaults at runtime
    let minimal_config =
        create_minimal_preset_config(&merged_config, preset_names, last_preset_config.as_ref());

    let config_yaml = serde_yaml::to_string(&minimal_config)?;
    let config_value = CoreOperations::parse_yaml_with_diagnostics(&config_yaml, "merged config")?;
    CoreOperations::write_yaml_file(&config_path, &config_value)?;

    let scope = if global { "global" } else { "local" };
    vm_success!(
        "{}",
        msg!(
            MESSAGES.config_preset_applied,
            preset = preset_names,
            path = scope
        )
    );

    let preset_list: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
    if preset_list.len() > 1 {
        vm_println!("{}", MESSAGES.config_applied_presets);
        for preset in preset_list {
            vm_println!("    • {}", preset);
        }
    }

    vm_println!("{}", MESSAGES.config_restart_hint);
    Ok(())
}

/// Create a minimal configuration that only includes project-specific fields.
///
/// The minimal config contains only what users should customize per-project:
/// - `preset`: Which preset to use (declared explicitly)
/// - `provider`: Which provider to use (required field)
/// - `project`: Project identity (name, hostname, workspace)
/// - `vm.box`: Base image/box (from preset if specified, otherwise merged)
/// - `ports`: Project-specific port allocation
/// - `services`: Which services this project needs
/// - `terminal`: Per-project terminal customization
///
/// Everything else (packages, versions, aliases, host_sync, os, etc.)
/// comes from the preset/defaults at runtime and shouldn't be written to vm.yaml.
fn create_minimal_preset_config(
    merged: &VmConfig,
    preset_names: &str,
    preset: Option<&VmConfig>,
) -> VmConfig {
    use crate::config::VmSettings;

    // Start with merged VM settings so required defaults (user, timezone, etc.) stay intact
    let mut vm = merged.vm.clone();

    // If preset specifies a box (e.g., '@vibe-base'), override the cloned value
    if let Some(preset_box) = preset
        .and_then(|p| p.vm.as_ref())
        .and_then(|vm| vm.r#box.clone())
    {
        if vm.is_none() {
            vm = Some(VmSettings::default());
        }
        if let Some(vm_settings) = vm.as_mut() {
            vm_settings.r#box = Some(preset_box);
        }
    }

    VmConfig {
        preset: Some(preset_names.to_string()),
        version: merged.version.clone(),
        provider: merged.provider.clone(), // Required field for validation
        project: merged.project.clone(),
        vm,
        ports: merged.ports.clone(),
        services: merged.services.clone(),
        terminal: merged.terminal.clone(),
        ..Default::default()
    }
}
