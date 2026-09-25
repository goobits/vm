use std::path::PathBuf;
use std::process::Command;

use super::{base, packages, uninstall, update};
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
            let controller_version = packages::installed_controller_version()?;
            let providers = ["docker", "podman", "tart"]
                .into_iter()
                .filter_map(|name| provider_version(name).map(|version| (name, version)))
                .collect::<std::collections::BTreeMap<_, _>>();
            if *json {
                crate::presentation::success(
                    "system info",
                    serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "client_version": env!("CARGO_PKG_VERSION"),
                        "controller_version": controller_version,
                        "providers": providers,
                        "config_schema_version": "2.0",
                        "output_schema_version": 1,
                        "executable": executable,
                        "managed_installation": installed.is_some(),
                        "installed_version": installed.as_ref().map(|record| record.version.as_str()),
                    }),
                )
            } else {
                vm_core::vm_println!("vm {}", env!("CARGO_PKG_VERSION"));
                vm_core::vm_println!(
                    "Package controller: {}",
                    controller_version.as_deref().unwrap_or("not configured")
                );
                for (name, version) in providers {
                    vm_core::vm_println!("{name}: {version}");
                }
                vm_core::vm_println!("Configuration schema: 2.0");
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

fn provider_version(name: &str) -> Option<String> {
    let output = Command::new(name).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()?
        .trim()
        .to_owned();
    (!line.is_empty()).then_some(line)
}
