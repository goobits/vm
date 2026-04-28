// Standard library
use std::path::PathBuf;

// External crates
use tracing::{error, info};
use vm_core::error::Result;

// Internal imports
use crate::{config::VmConfig, loader::ConfigLoader};

pub fn execute_validate(file: Option<PathBuf>, verbose: bool) {
    match load_and_merge_config(file) {
        Ok(_) => {
            info!("✅ Configuration is valid");
            if verbose {
                info!("Successfully loaded the configuration.");
            }
        }
        Err(e) => {
            error!("❌ Configuration validation failed: {:#}", e);
            std::process::exit(1);
        }
    }
}

#[must_use = "file validation results should be handled"]
pub fn execute_check_file(file: PathBuf) -> Result<()> {
    use crate::yaml::YamlOperations;
    match YamlOperations::validate_file(&file) {
        Ok(_) => {
            info!("✅ File is valid YAML");
            std::process::exit(0);
        }
        Err(e) => {
            error!("❌ File validation failed: {}", e);
            std::process::exit(1);
        }
    }
}

use vm_core::error::VmError;

#[must_use = "configuration loading results should be used"]
pub fn load_and_merge_config(file: Option<PathBuf>) -> Result<VmConfig> {
    let loader = ConfigLoader::new();
    let result = if let Some(path) = file {
        loader.load_from_path(&path)
    } else {
        loader.load()
    };

    let config = result.map_err(|e| VmError::Config(e.to_string()))?;
    apply_declared_presets(config)
}

fn apply_declared_presets(config: VmConfig) -> Result<VmConfig> {
    let Some(preset_names) = config.preset.clone() else {
        return Ok(config);
    };

    let source_path = config.source_path.clone();
    let project_dir = std::env::current_dir()
        .map_err(|e| VmError::Config(format!("Failed to resolve current directory: {e}")))?;
    let detector = crate::preset::PresetDetector::new(project_dir, crate::paths::get_presets_dir());

    let port_range_str = config.ports.range.as_ref().and_then(|range| {
        if range.len() == 2 {
            Some(format!("{}-{}", range[0], range[1]))
        } else {
            None
        }
    });

    let mut merged_preset = VmConfig::default();
    for preset_name in preset_names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let preset_config = crate::config_ops::port_placeholders::load_preset_with_placeholders(
            &detector,
            preset_name,
            &port_range_str,
        )
        .map_err(|e| VmError::Config(format!("Failed to load preset '{preset_name}': {e}")))?;
        merged_preset = crate::merge::ConfigMerger::new(merged_preset).merge(preset_config)?;
    }

    let mut merged = crate::merge::ConfigMerger::new(merged_preset).merge(config)?;
    merged.source_path = source_path;
    Ok(merged)
}
