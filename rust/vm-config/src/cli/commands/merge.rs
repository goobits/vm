use std::path::PathBuf;
use vm_core::error::{Result, VmError};

use crate::cli::formatting::output_config;
use crate::cli::OutputFormat;
use crate::{config::VmConfig, merge};

pub fn execute_merge(base: PathBuf, overlay: Vec<PathBuf>, format: OutputFormat) -> Result<()> {
    let base_config = VmConfig::from_file(&base)
        .map_err(|e| VmError::Config(format!("Failed to load base config: {base:?}: {e}")))?;

    let mut overlays = Vec::new();
    for path in overlay {
        let config = VmConfig::from_file(&path)
            .map_err(|e| VmError::Config(format!("Failed to load overlay: {path:?}: {e}")))?;
        overlays.push(config);
    }

    let merged = merge::ConfigMerger::new(base_config).merge_all(overlays)?;
    output_config(&merged, &format)
}

pub fn execute_merge_eval_all(files: Vec<PathBuf>, format: OutputFormat) -> Result<()> {
    use crate::yaml::YamlOperations;
    YamlOperations::merge_eval_all(&files, &format)
}
