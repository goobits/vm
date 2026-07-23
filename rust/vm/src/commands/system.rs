use std::path::PathBuf;

use crate::cli::SystemSubcommand;
use crate::error::VmResult;
use vm_config::AppConfig;

use super::{base, registry, uninstall, update};

pub(super) async fn handle(
    command: &SystemSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        SystemSubcommand::Update { version, force } => {
            update::handle_update(version.as_deref(), *force)
        }
        SystemSubcommand::Uninstall { keep_config, yes } => {
            uninstall::handle_uninstall(*keep_config, *yes)
        }
        SystemSubcommand::Registry { command } => {
            let global_config = AppConfig::load(config_path, profile, None)
                .map(|config| config.global)
                .unwrap_or_default();
            registry::handle_registry_command(command, global_config).await
        }
        SystemSubcommand::Base { command } => base::handle_base(command.clone()).await,
    }
}
