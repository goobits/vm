// Preset-related command handlers

use anyhow::Result;

use crate::cli::PresetSubcommand;
use vm_config::ConfigOps;

/// Handle preset commands
pub fn handle_preset_command(command: &PresetSubcommand) -> Result<()> {
    match command {
        PresetSubcommand::List => ConfigOps::preset("", false, true, None),
        PresetSubcommand::Show { name } => ConfigOps::preset("", false, false, Some(name)),
    }
}
