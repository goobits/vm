// Temporary VM command handlers

use anyhow::Result;
use std::path::PathBuf;

use crate::cli::TempSubcommand;
use crate::commands::config;
use vm_provider::get_provider;

/// Handle temporary VM commands
pub fn handle_temp_command(
    command: &TempSubcommand,
    config_file: Option<PathBuf>,
) -> Result<()> {
    use vm_temp::TempVmOps;

    // For temp commands, we need a provider, but the config might not exist.
    // We load it leniently to ensure we can get a provider.
    let config = config::load_config_lenient(config_file)?;
    let provider = get_provider(config.clone())?;

    match command {
        TempSubcommand::Create {
            mounts,
            auto_destroy,
        } => TempVmOps::create(mounts.clone(), *auto_destroy, config, provider),
        TempSubcommand::Ssh => TempVmOps::ssh(provider),
        TempSubcommand::Status => TempVmOps::status(provider),
        TempSubcommand::Destroy => TempVmOps::destroy(provider),
        TempSubcommand::Mount { path, yes } => TempVmOps::mount(path.clone(), *yes, provider),
        TempSubcommand::Unmount { path, all, yes } => {
            TempVmOps::unmount(path.clone(), *all, *yes, provider)
        }
        TempSubcommand::Mounts => TempVmOps::mounts(),
        TempSubcommand::List => TempVmOps::list(),
        TempSubcommand::Stop => TempVmOps::stop(provider),
        TempSubcommand::Start => TempVmOps::start(provider),
        TempSubcommand::Restart => TempVmOps::restart(provider),
    }
}
