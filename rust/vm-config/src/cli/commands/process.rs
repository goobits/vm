use anyhow::{Context, Result};
use std::path::PathBuf;

use super::super::{utils::output_config, OutputFormat};
use crate::{config::VmConfig, merge, paths, preset::PresetDetector};

pub fn execute(
    defaults: Option<PathBuf>,
    config: Option<PathBuf>,
    project_dir: PathBuf,
    presets_dir: Option<PathBuf>,
    format: OutputFormat,
) -> Result<()> {
    // Use default paths if not specified
    let defaults = defaults.unwrap_or_else(|| paths::resolve_tool_path("vm.yaml"));
    let presets_dir = presets_dir.unwrap_or_else(paths::get_presets_dir);

    // Load default config
    let default_config = VmConfig::from_file(&defaults)
        .with_context(|| format!("Failed to load defaults: {:?}", defaults))?;

    // Load user config if provided
    let user_config = if let Some(path) = config {
        Some(
            VmConfig::from_file(&path)
                .with_context(|| format!("Failed to load user config: {:?}", path))?,
        )
    } else {
        None
    };

    // Detect and load preset if user config is partial
    let preset_config = if user_config.as_ref().is_none_or(|c| c.is_partial()) {
        let detector = PresetDetector::new(project_dir, presets_dir);
        if let Some(preset_name) = detector.detect()? {
            Some(detector.load_preset(&preset_name)?)
        } else {
            None
        }
    } else {
        None
    };

    // Merge in order: defaults -> global -> preset -> user
    let global_config = crate::config_ops::load_global_config();
    let merged = merge::merge_configs(
        Some(default_config),
        global_config,
        preset_config,
        user_config,
    )?;
    output_config(&merged, &format)
}