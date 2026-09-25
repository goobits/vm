mod materialize;

// External crates
use serde_yaml_ng as serde_yaml;
use std::collections::BTreeSet;
use std::path::PathBuf;
use tracing::instrument;

// Internal imports
use crate::config::VmConfig;
use crate::config_ops::io::{get_global_config_path, get_or_create_global_config_path};
use crate::config_ops::plan::{ConfigEditPlan, ConfigMutationReport};
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
pub fn preset(
    preset_names: &str,
    global: bool,
    list: bool,
    show: Option<&str>,
    dry_run: bool,
    path: Option<PathBuf>,
    structured: bool,
) -> Result<Option<ConfigMutationReport>> {
    let project_dir = match path.as_ref().and_then(|path| path.parent()) {
        Some(parent) => parent.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let detector = PresetDetector::new(project_dir);

    if list {
        return list_presets(&detector).map(|_| None);
    }

    if let Some(name) = show {
        return show_preset(&detector, name).map(|_| None);
    }

    apply_preset_to_config(&detector, preset_names, global, dry_run, path, structured)
}

pub(super) fn apply_report(
    preset_names: &str,
    global: bool,
    dry_run: bool,
    path: Option<PathBuf>,
) -> Result<ConfigMutationReport> {
    if !global && !path.as_ref().is_some_and(|path| path.is_file()) {
        return Err(VmError::Config(
            "A project configuration must exist for structured preset application".to_string(),
        ));
    }
    let project_dir = match path.as_ref().and_then(|path| path.parent()) {
        Some(parent) => parent.to_path_buf(),
        None => std::env::current_dir()?,
    };
    let detector = PresetDetector::new(project_dir);
    apply_preset_to_config(&detector, preset_names, global, dry_run, path, true)?.ok_or_else(|| {
        VmError::Config("Preset application did not produce a configuration plan".to_string())
    })
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
    dry_run: bool,
    path: Option<PathBuf>,
    structured: bool,
) -> Result<Option<ConfigMutationReport>> {
    let local_config_path = if global {
        None
    } else {
        Some(path.unwrap_or(std::env::current_dir()?.join("vm.yaml")))
    };
    let initializing_local = local_config_path
        .as_ref()
        .is_some_and(|path| !path.exists());

    // Validate all presets exist BEFORE attempting to initialize/modify config
    let preset_list: Vec<&str> = preset_names.split(',').map(|s| s.trim()).collect();
    let available_presets = if initializing_local {
        detector.list_all_presets()?
    } else {
        detector.list_presets()?
    };
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
        vm_println!("💡 Apply with: vm config presets apply <name>");
        return Err(VmError::Config(format!(
            "Preset(s) not found: {}",
            missing_presets.join(", ")
        )));
    }

    // A new project should use the canonical initialization path. In particular,
    // image presets replace base provisioning fields instead of being merged over
    // them as though they were ordinary provision presets.
    if initializing_local && dry_run {
        return Err(VmError::Config(
            "Cannot preview preset application without an existing vm.yaml. Run `vm init` first"
                .to_string(),
        ));
    }
    if initializing_local && preset_list.len() == 1 {
        let config_path = local_config_path.expect("new local config path should exist");
        vm_println!("⚠️  No vm.yaml found. Initializing project first...");
        vm_println!("");
        super::init_config_file(
            Some(config_path),
            None,
            None,
            Some(preset_list[0].to_string()),
        )?;
        vm_println!("");
        vm_success!(
            "{}",
            msg!(
                MESSAGES.config.preset_applied,
                preset = preset_names,
                path = "local"
            )
        );
        vm_println!("{}", MESSAGES.config.restart_hint);
        return Ok(None);
    }

    let config_path = if global {
        get_global_config_path()
    } else {
        // For preset command, only look in current directory, not parent directories
        // This ensures we create vm.yaml in the current project, not modify a parent config
        local_config_path.expect("local config path should exist")
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
    let explicit_value = if config_existed {
        let content = std::fs::read_to_string(&config_path)?;
        CoreOperations::parse_yaml_with_diagnostics(&content, &config_path.display().to_string())?
    } else {
        serde_yaml::Value::Null
    };
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

    let mut conflicts = BTreeSet::new();
    collect_changed_explicit_fields(
        &explicit_value,
        &serde_yaml::to_value(&original_base_config)?,
        &serde_yaml::to_value(&merged_config)?,
        "",
        &mut conflicts,
    );
    if !conflicts.is_empty() {
        return Err(VmError::Config(format!(
            "Preset conflicts with explicit settings: {}. Unset those fields before applying it",
            conflicts.into_iter().collect::<Vec<_>>().join(", ")
        )));
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
    if warn_preserved_customizations && !dry_run && !structured {
        print_customization_warning(&original_base_config);
    }

    let config_yaml = serde_yaml::to_string(&minimal_config)?;
    let config_value = CoreOperations::parse_yaml_with_diagnostics(&config_yaml, "merged config")?;
    super::validate::candidate(&config_value, &config_path, global)?;
    let before = if config_existed {
        explicit_value
    } else {
        serde_yaml::Value::Mapping(serde_yaml::Mapping::new())
    };
    let plan = ConfigEditPlan::new(config_path.clone(), before, config_value, global);
    let original_effective = serde_yaml::to_value(&original_base_config)?;
    let merged_effective = serde_yaml::to_value(&merged_config)?;
    let effective = Some((&original_effective, &merged_effective));
    if dry_run {
        if !structured {
            plan.preview_with_effective(&original_effective, &merged_effective);
        }
        return Ok(Some(plan.report(true, effective)));
    }
    if global {
        let _ = get_or_create_global_config_path()?;
    }
    plan.write()?;

    if !structured {
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
    }
    Ok(Some(plan.report(false, effective)))
}

fn collect_changed_explicit_fields(
    explicit: &serde_yaml::Value,
    before: &serde_yaml::Value,
    after: &serde_yaml::Value,
    path: &str,
    conflicts: &mut BTreeSet<String>,
) {
    if let serde_yaml::Value::Mapping(fields) = explicit {
        for (key, value) in fields {
            let Some(key) = key.as_str() else { continue };
            let next = if path.is_empty() {
                key.to_string()
            } else {
                format!("{path}.{key}")
            };
            if matches!(next.as_str(), "preset" | "version" | "$schema") {
                continue;
            }
            let before_field = before.as_mapping().and_then(|map| map.get(key));
            let after_field = after.as_mapping().and_then(|map| map.get(key));
            match (before_field, after_field) {
                (Some(before_field), Some(after_field)) => collect_changed_explicit_fields(
                    value,
                    before_field,
                    after_field,
                    &next,
                    conflicts,
                ),
                (Some(_), None) => {
                    conflicts.insert(next);
                }
                _ => {}
            }
        }
    } else if before != after && !path.is_empty() {
        conflicts.insert(path.to_string());
    }
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
