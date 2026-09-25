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
        SystemSubcommand::Info { json } => {
            let executable = std::env::current_exe()?;
            let installed = vm_core::install_record::verified(&executable).ok();
            if *json {
                crate::presentation::success(
                    "system info",
                    serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "executable": executable,
                        "managed_installation": installed.is_some(),
                        "installed_version": installed.as_ref().map(|record| record.version.as_str()),
                    }),
                )
            } else {
                vm_core::vm_println!("vm {}", env!("CARGO_PKG_VERSION"));
                vm_core::vm_println!("Executable: {}", executable.display());
                vm_core::vm_println!(
                    "Managed installation: {}",
                    if installed.is_some() { "yes" } else { "no" }
                );
                Ok(())
            }
        }
        SystemSubcommand::Update { version } => update::handle_update(version.as_deref()),
        SystemSubcommand::Uninstall {
            delete_config,
            delete_data,
            yes,
        } => uninstall::handle_uninstall(*delete_config, *delete_data, *yes),
        SystemSubcommand::Images { command } => base::handle_base(command.clone()).await,
        SystemSubcommand::Storage { command } => storage::handle(command),
    }
}
