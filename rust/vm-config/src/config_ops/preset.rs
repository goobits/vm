mod materialize;

// External crates
use serde_yaml_ng as serde_yaml;
use tracing::instrument;

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::get_or_create_global_config_path;
use crate::config_ops::port_placeholders::load_preset_with_placeholders;
use crate::merge::ConfigMerger;
use crate::preset::PresetDetector;
use crate::yaml::core::CoreOperations;
use vm_core::error::{Result, VmError};
use vm_core::msg;
use vm_core::{vm_println, vm_success};
use vm_messages::messages::MESSAGES;

use materialize::materialize_minimal_preset_config;
pub(crate) use materialize::resolve_declared_presets;

/// Apply preset(s) to configuration
pub fn preset(preset_names: &str, global: bool, list: bool, show: Option<&str>) -> Result<()> {
    let project_dir = std::env::current_dir()?;
    let detector = PresetDetector::new(project_dir);

    if list {
        return list_presets(&detector);
    }

    if let Some(name) = show {
        return show_preset(&detector, name);
    }

    apply_preset_to_config(&detector, preset_names, global)
}

/// List all available presets
fn list_presets(detector: &PresetDetector) -> Result<()> {
    let presets = detector.list_presets()?;
    vm_println!("{}", MESSAGES.config.available_presets);
    for preset in presets {
        let description = detector
            .get_preset_description(&preset)
            .map(|d| format!(" - {d}"))
            .unwrap_or_default();
        vm_println!("  • {}{}", preset, description);
    }
    vm_println!(
        "{}",
        msg!(MESSAGES.config.apply_preset_hint, name = "<name>")
    );
    Ok(())
}

/// Show a specific preset configuration
fn show_preset(detector: &PresetDetector, name: &str) -> Result<()> {
    let preset_config = detector.load_preset(name)?;
    let yaml = serde_yaml::to_string(&preset_config)?;
    vm_println!("📋 Preset '{}' configuration:\n", name);
    vm_println!("{}", yaml);
    vm_println!("{}", msg!(MESSAGES.config.apply_preset_hint, name = name));
    Ok(())
}

/// Apply preset(s) to configuration
#[instrument(skip(detector), fields(preset_names = %preset_names, global = %global))]
fn apply_preset_to_config(
    detector: &PresetDetector,
    preset_names: &str,
    global: bool,
) -> Result<()> {
    // Validate all presets exist BEFORE attempting to initialize/modify config
    let preset_list: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
    let available_presets = detector.list_presets()?;
    let mut missing_presets = Vec::new();

    for preset_name in &preset_list {
        if !available_presets.contains(&preset_name.to_string()) {
            missing_presets.push(*preset_name);
        }
    }

    if !missing_presets.is_empty() {
        vm_println!("❌ Preset(s) not found: {}", missing_presets.join(", "));
        vm_println!("");
        vm_println!("📦 Available presets:");
        for preset in available_presets {
            let description = detector
                .get_preset_description(&preset)
                .map(|d| format!(" - {d}"))
                .unwrap_or_default();
            vm_println!("  • {}{}", preset, description);
        }
        vm_println!("");
        vm_println!("💡 Apply with: vm config preset <name>");
        return Err(VmError::Config(format!(
            "Preset(s) not found: {}",
            missing_presets.join(", ")
        )));
    }

    let config_path = if global {
        get_or_create_global_config_path()?
    } else {
        // For preset command, only look in current directory, not parent directories
        // This ensures we create vm.yaml in the current project, not modify a parent config
        std::env::current_dir()?.join("vm.yaml")
    };

    // Track if config already existed (for Bug #2 - preserve user customizations)
    let config_existed = config_path.exists();
    let mut called_init = false;

    let base_config = if global {
        if config_existed {
            let content = std::fs::read_to_string(&config_path)?;
            let source_desc = format!("{}", config_path.display());
            CoreOperations::parse_yaml_with_diagnostics(&content, &source_desc)?
        } else {
            VmConfig::default()
        }
    } else {
        // If no vm.yaml exists, reuse the canonical initialization path.
        if !config_existed {
            vm_println!("⚠️  No vm.yaml found. Initializing project first...");
            vm_println!("");
            super::init_config_file(Some(config_path.clone()), None, None, None)?;
            vm_println!("");
            called_init = true;
        }
        // Now load the config (either existing or just created by init)
        VmConfig::from_file(&config_path)?
    };

    let preset_iter = preset_names.split(',').map(|s| s.trim());

    // Clone base_config to track original user customizations
    let original_base_config = base_config.clone();
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

        let preset_config = load_preset_with_placeholders(detector, preset_name, &port_range_str)
            .map_err(|e| {
            VmError::Config(format!("Failed to load preset: {preset_name}: {e}"))
        })?;

        merged_config = ConfigMerger::new(merged_config).merge(preset_config.clone())?;
        last_preset_config = Some(preset_config);
    }

    // Create minimal config with only project-specific fields
    let (minimal_config, warn_preserved_customizations) = materialize_minimal_preset_config(
        &merged_config,
        preset_names,
        last_preset_config.as_ref(),
        if config_existed || called_init {
            Some(&original_base_config)
        } else {
            None
        },
        called_init,
    );
    if warn_preserved_customizations {
        print_customization_warning(&original_base_config);
    }

    let config_yaml = serde_yaml::to_string(&minimal_config)?;
    let config_value = CoreOperations::parse_yaml_with_diagnostics(&config_yaml, "merged config")?;
    CoreOperations::write_yaml_file(&config_path, &config_value)?;

    let scope = if global { "global" } else { "local" };
    vm_success!(
        "{}",
        msg!(
            MESSAGES.config.preset_applied,
            preset = preset_names,
            path = scope
        )
    );

    let preset_list: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
    if preset_list.len() > 1 {
        vm_println!("{}", MESSAGES.config.applied_presets);
        for preset in preset_list {
            vm_println!("    • {}", preset);
        }
    }

    vm_println!("{}", MESSAGES.config.restart_hint);
    Ok(())
}

/// Print warning about preserved customizations
fn print_customization_warning(original: &VmConfig) {
    vm_println!("");
    vm_println!(
        "⚠️  Note: Your vm.yaml contains customizations that are typically defined in presets:"
    );

    if original.versions.is_some() {
        vm_println!("   - versions (node, python, etc.)");
    }
    if !original.storage.is_empty() {
        vm_println!("   - storage policy");
    }
    if !original.mounts.is_empty() {
        vm_println!("   - mounts");
    }
    if !crate::config::ToolsConfig::is_empty(&original.tools) {
        vm_println!("   - tools");
    }
    if original.bootstrap.is_some() {
        vm_println!("   - project bootstrap policy");
    }
    if !original.apt_packages.is_empty()
        || !original.npm_packages.is_empty()
        || !original.pip_packages.is_empty()
        || !original.cargo_packages.is_empty()
    {
        vm_println!("   - packages (apt, npm, pip, cargo)");
    }
    if !original.aliases.is_empty() {
        vm_println!("   - aliases");
    }
    if !original.environment.is_empty() {
        vm_println!("   - environment variables");
    }
    if original.host_sync.is_some() {
        vm_println!("   - host_sync settings");
    }
    if original.networking.is_some() {
        vm_println!("   - networking config");
    }
    if original.default_profile.is_some() || original.profiles.is_some() {
        vm_println!("   - profile configuration");
    }
    if original.tart.is_some() {
        vm_println!("   - tart provider settings");
    }

    vm_println!("");
    vm_println!("These have been preserved, but consider:");
    vm_println!("   • Moving global preferences to ~/.vm/config.yaml");
    vm_println!("   • Creating a custom preset for reusable configurations");
    vm_println!("   • See: https://github.com/goobits/vm#presets");
    vm_println!("");
}
