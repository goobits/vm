// External crates
use serde_yaml_ng as serde_yaml;

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::{find_or_create_local_config, get_or_create_global_config_path};
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
        return Ok(());
    }

    if let Some(name) = show {
        let preset_config = detector.load_preset(name)?;
        let yaml = serde_yaml::to_string(&preset_config)?;
        vm_println!("📋 Preset '{}' configuration:\n", name);
        vm_println!("{}", yaml);
        vm_println!("{}", msg!(MESSAGES.config.apply_preset_hint, name = name));
        return Ok(());
    }

    let config_path = if global {
        get_or_create_global_config_path()?
    } else {
        find_or_create_local_config()?
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
        // Bug #1 fix: If no vm.yaml exists, run full init first (reuse vm init logic)
        if !config_existed {
            vm_println!("⚠️  No vm.yaml found. Initializing project first...");
            vm_println!("");
            crate::cli::init_config_file(Some(config_path.clone()), None, None)?;
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

        let preset_config = load_preset_with_placeholders(&detector, preset_name, &port_range_str)
            .map_err(|e| VmError::Config(format!("Failed to load preset: {preset_name}: {e}")))?;

        merged_config = ConfigMerger::new(merged_config).merge(preset_config.clone())?;
        last_preset_config = Some(preset_config);
    }

    // Create minimal config with only project-specific fields
    // Bug #2 fix: Preserve user customizations if config existed OR we just called init
    // If we just called init, preserve what it created (don't wastefully strip it)
    // Everything else (packages, versions, aliases, host_sync, etc.) comes from preset/defaults at runtime
    let minimal_config = create_minimal_preset_config(
        &merged_config,
        preset_names,
        last_preset_config.as_ref(),
        if config_existed || called_init {
            Some(&original_base_config)
        } else {
            None
        },
        called_init, // Don't warn if we just created the config
    );

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
///
/// Bug fix: If `original_base_config` is provided (existing vm.yaml), preserve user
/// customizations even if they're "non-standard" (packages, versions, aliases, etc.)
///
/// The `suppress_warning` flag prevents showing the customization warning when we just
/// created the config via init (avoids confusing UX).
fn create_minimal_preset_config(
    merged: &VmConfig,
    preset_names: &str,
    preset: Option<&VmConfig>,
    original_base_config: Option<&VmConfig>,
    suppress_warning: bool,
) -> VmConfig {
    use crate::config::VmSettings;

    // Start with merged VM settings so required defaults (user, timezone, etc.) stay intact
    let mut vm = merged.vm.clone();

    // If preset specifies a box (e.g., '@vibe-box'), override the cloned value
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

    // Start with minimal config
    let mut minimal = VmConfig {
        preset: Some(preset_names.to_string()),
        version: merged.version.clone(),
        provider: merged.provider.clone(), // Required field for validation
        project: merged.project.clone(),
        vm,
        ports: merged.ports.clone(),
        services: merged.services.clone(),
        terminal: merged.terminal.clone(),
        apt_packages: merged.apt_packages.clone(),
        npm_packages: merged.npm_packages.clone(),
        pip_packages: merged.pip_packages.clone(),
        cargo_packages: merged.cargo_packages.clone(),
        ..Default::default()
    };

    // Bug #2 fix: Preserve user customizations if they existed before
    if let Some(original) = original_base_config {
        let mut has_customizations = false;

        // Preserve versions if user had them
        if original.versions.is_some() {
            minimal.versions = merged.versions.clone();
            has_customizations = true;
        }

        // Preserve apt packages if user had them
        if !original.apt_packages.is_empty() {
            minimal.apt_packages = merged.apt_packages.clone();
            has_customizations = true;
        }

        // Preserve npm packages if user had them
        if !original.npm_packages.is_empty() {
            minimal.npm_packages = merged.npm_packages.clone();
            has_customizations = true;
        }

        // Preserve pip packages if user had them
        if !original.pip_packages.is_empty() {
            minimal.pip_packages = merged.pip_packages.clone();
            has_customizations = true;
        }

        // Preserve cargo packages if user had them
        if !original.cargo_packages.is_empty() {
            minimal.cargo_packages = merged.cargo_packages.clone();
            has_customizations = true;
        }

        // Preserve aliases if user had them
        if !original.aliases.is_empty() {
            minimal.aliases = merged.aliases.clone();
            has_customizations = true;
        }

        // Preserve environment if user had it
        if !original.environment.is_empty() {
            minimal.environment = merged.environment.clone();
            has_customizations = true;
        }

        // Preserve host_sync if user had it
        if original.host_sync.is_some() {
            minimal.host_sync = merged.host_sync.clone();
            has_customizations = true;
        }

        // Preserve os if user had it
        if original.os.is_some() {
            minimal.os = merged.os.clone();
            has_customizations = true;
        }

        // Preserve networking if user had it
        if original.networking.is_some() {
            minimal.networking = merged.networking.clone();
            has_customizations = true;
        }

        // Warn user about preserved customizations (but not if we just created the config)
        if has_customizations && !suppress_warning {
            vm_println!("");
            vm_println!("⚠️  Note: Your vm.yaml contains customizations that are typically defined in presets:");
            if original.versions.is_some() {
                vm_println!("   - versions (node, python, etc.)");
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
            vm_println!("");
            vm_println!("These have been preserved, but consider:");
            vm_println!("   • Moving global preferences to ~/.vm/config.yaml");
            vm_println!("   • Creating a custom preset for reusable configurations");
            vm_println!("   • See: https://github.com/goobits/vm#presets");
            vm_println!("");
        }
    }

    minimal
}
