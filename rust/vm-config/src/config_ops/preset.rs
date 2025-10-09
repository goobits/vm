// External crates
use serde_yaml_ng as serde_yaml;

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::{find_or_create_local_config, get_or_create_global_config_path, read_config_or_init};
use crate::config_ops::port_placeholders::load_preset_with_placeholders;
use crate::merge::ConfigMerger;
use crate::paths;
use crate::preset::PresetDetector;
use crate::yaml::core::CoreOperations;
use vm_cli::msg;
use vm_core::error::{Result, VmError};
use vm_core::vm_println;
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
                .map(|d| format!(" - {}", d))
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
            serde_yaml::from_str(&content)?
        } else {
            VmConfig::default()
        }
    } else {
        read_config_or_init(&config_path, true)?
    };

    let preset_iter = preset_names.split(',').map(|s| s.trim());
    let mut merged_config = base_config;

    for preset_name in preset_iter {
        let port_range_str = merged_config.ports.range.as_ref().and_then(|range| {
            if range.len() == 2 {
                Some(format!("{}-{}", range[0], range[1]))
            } else {
                None
            }
        });

        let preset_config =
            load_preset_with_placeholders(&detector, preset_name, &port_range_str).map_err(
                |e| VmError::Config(format!("Failed to load preset: {}: {}", preset_name, e)),
            )?;

        merged_config = ConfigMerger::new(merged_config).merge(preset_config)?;
    }

    let config_yaml = serde_yaml::to_string(&merged_config)?;
    let config_value = serde_yaml::from_str(&config_yaml)?;
    CoreOperations::write_yaml_file(&config_path, &config_value)?;

    let scope = if global { "global" } else { "local" };
    vm_println!(
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