use std::path::PathBuf;

use super::{base, uninstall, update};
use crate::cli::SystemSubcommand;
use crate::error::VmResult;

#[path = "system_storage.rs"]
mod storage;

pub(super) async fn handle(
    command: &SystemSubcommand,
    _config_path: Option<PathBuf>,
    _profile: Option<String>,
) -> VmResult<()> {
    match command {
        SystemSubcommand::Info => {
            let executable = std::env::current_exe()?;
            vm_core::vm_println!("vm {}", env!("CARGO_PKG_VERSION"));
            vm_core::vm_println!("Executable: {}", executable.display());
            Ok(())
        }
        SystemSubcommand::Update { version } => update::handle_update(version.as_deref(), false),
        SystemSubcommand::Uninstall { delete_config, yes } => {
            uninstall::handle_uninstall(!delete_config, *yes)
        }
        SystemSubcommand::Images { command } => base::handle_base(command.clone()).await,
        SystemSubcommand::Storage { command } => storage::handle(command),
    }
}
