use std::path::PathBuf;
use vm_core::error::Result;

use crate::cli::commands;
use crate::cli::OutputFormat;

/// Handle all project operation commands
pub struct ProjectOpsGroup;

impl ProjectOpsGroup {
    /// Execute preset command
    pub fn execute_preset(
        dir: PathBuf,
        presets_dir: Option<PathBuf>,
        detect_only: bool,
        list: bool,
    ) -> Result<()> {
        commands::preset::execute(dir, presets_dir, detect_only, list)
    }

    /// Execute process command
    pub fn execute_process(
        defaults: Option<PathBuf>,
        config: Option<PathBuf>,
        project_dir: PathBuf,
        presets_dir: Option<PathBuf>,
        format: OutputFormat,
    ) -> Result<()> {
        commands::process::execute(defaults, config, project_dir, presets_dir, format)
    }

    /// Execute init command
    pub fn execute_init(
        file: Option<PathBuf>,
        services: Option<String>,
        ports: Option<u16>,
        preset: Option<String>,
    ) -> Result<()> {
        commands::init::execute(file, services, ports, preset)
    }
}
