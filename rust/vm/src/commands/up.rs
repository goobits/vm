//! VM Up command - Zero to code in one command
//!
//! This command orchestrates: init → create → start → ssh
//! It's idempotent - safe to run multiple times.

use crate::commands::{init, vm_ops};
use crate::error::{VmError, VmResult};
use std::path::Path;
use tracing::debug;
use vm_config::{
    config::{ProjectConfig, VmConfig},
    detector::detect_project_name,
    resources::detect_resource_defaults,
    AppConfig,
};
use vm_core::{vm_println, vm_success};
use vm_provider::get_provider;

/// Handle the `vm up` command
///
/// This command provides a single entry point to go from nothing to a running
/// VM with an SSH session. It handles:
/// 1. Creating vm.yaml if it doesn't exist
/// 2. Creating the VM if it doesn't exist
/// 3. Starting the VM if it's stopped
/// 4. Opening an SSH session (or executing a command)
pub async fn handle_up(
    config_path: Option<std::path::PathBuf>,
    command: Option<String>,
    profile: Option<String>,
) -> VmResult<()> {
    debug!("Handling vm up command");

    // Stage 1: Check if vm.yaml exists, create if not
    let config_file = config_path
        .clone()
        .unwrap_or_else(|| Path::new("vm.yaml").to_path_buf());

    if !config_file.exists() {
        vm_println!("📝 No vm.yaml found, initializing...");
        init::handle_init(None, None, None, None)?;
        vm_success!("✓ Created vm.yaml");
    }

    // Stage 2: Load configuration
    let app_config = match AppConfig::load(config_path.clone(), profile.clone()) {
        Ok(config) => config,
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("No vm.yaml found") {
                // This shouldn't happen after init, but handle gracefully
                vm_println!("📝 Generating default configuration...");
                let resources = detect_resource_defaults();
                let default_vm_config = VmConfig {
                    provider: Some("docker".to_string()),
                    project: Some(ProjectConfig {
                        name: Some(detect_project_name()?),
                        ..Default::default()
                    }),
                    vm: Some(vm_config::config::VmSettings {
                        memory: Some(vm_config::config::MemoryLimit::Limited(resources.memory)),
                        cpus: Some(vm_config::config::CpuLimit::Limited(resources.cpus)),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                default_vm_config.write_to_file(&config_file)?;
                AppConfig::load(config_path, profile)?
            } else {
                return Err(VmError::from(e));
            }
        }
    };

    let config = app_config.vm.clone();
    let global_config = app_config.global.clone();

    // Get provider
    let provider = get_provider(config.clone()).map_err(VmError::from)?;

    // Stage 3: Check VM status and create/start as needed
    let project_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.clone())
        .unwrap_or_else(|| "vm-project".to_string());

    // Try to get status report - this tells us if VM exists and its state
    let status_report = provider.get_status_report(None);
    debug!("Current VM status report: {:?}", status_report);

    match status_report {
        Ok(report) if report.is_running => {
            vm_println!("✓ VM '{}' is already running", project_name);
        }
        Ok(_report) => {
            // VM exists but is stopped
            vm_println!("🚀 Starting VM '{}'...", project_name);
            vm_ops::handle_start(
                get_provider(config.clone()).map_err(VmError::from)?,
                None,
                config.clone(),
                global_config.clone(),
            )
            .await?;
        }
        Err(_) => {
            // VM doesn't exist, create it
            vm_println!("🚀 Creating VM '{}'...", project_name);
            vm_ops::handle_create(
                get_provider(config.clone()).map_err(VmError::from)?,
                config.clone(),
                global_config.clone(),
                false, // force
                None,  // instance
                false, // verbose
                None,  // save_as
                None,  // from_dockerfile
                true,  // preserve_services
                false, // refresh_packages
            )
            .await?;
        }
    }

    // Stage 4: SSH in or execute command
    let provider = get_provider(config.clone()).map_err(VmError::from)?;

    if let Some(cmd) = command {
        debug!("Executing command: {}", cmd);
        vm_ops::handle_ssh(
            provider,
            None,
            None,
            Some(vec!["/bin/bash".to_string(), "-c".to_string(), cmd]),
            config,
            false,
            false,
        )
    } else {
        debug!("Opening interactive SSH session");
        vm_ops::handle_ssh(provider, None, None, None, config, false, false)
    }
}
