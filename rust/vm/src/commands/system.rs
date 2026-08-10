use std::path::PathBuf;

use super::{base, uninstall, update};
use crate::cli::SystemSubcommand;
use crate::error::VmResult;

pub(super) async fn handle(
    command: &SystemSubcommand,
    _config_path: Option<PathBuf>,
    _profile: Option<String>,
) -> VmResult<()> {
    match command {
        SystemSubcommand::Update { version, force } => {
            update::handle_update(version.as_deref(), *force)
        }
        SystemSubcommand::Uninstall { keep_config, yes } => {
            uninstall::handle_uninstall(*keep_config, *yes)
        }
        SystemSubcommand::Base { command } => base::handle_base(command.clone()).await,
    }
}
