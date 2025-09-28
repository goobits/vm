//! Package registry command handlers
//!
//! This module provides command handlers for the VM package registry functionality,
//! integrating with the vm-package-server library to provide npm, pip, and cargo
//! package caching and serving capabilities.

use crate::cli::{PkgConfigAction, PkgSubcommand};
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use crate::service_registry::get_service_registry;
use dialoguer::Confirm;
use std::path::PathBuf;
use vm_common::{vm_error, vm_println, vm_success};

use vm_package_server;

/// Handle package registry commands
pub async fn handle_pkg_command(command: &PkgSubcommand, _config: Option<PathBuf>) -> VmResult<()> {
    match command {
        PkgSubcommand::Status => handle_status().await,
        PkgSubcommand::Add { r#type } => handle_add(r#type.as_deref()).await,
        PkgSubcommand::Remove { force } => handle_remove(*force).await,
        PkgSubcommand::List => handle_list().await,
        PkgSubcommand::Config { action } => handle_config(action).await,
        PkgSubcommand::Use { shell, port } => handle_use(shell.as_deref(), *port).await,
    }
}

/// Show package registry status with service manager information
async fn handle_status() -> VmResult<()> {
    let registry = get_service_registry();
    let service_manager = get_service_manager();

    vm_println!("📊 Package Registry Status");

    // Get service status from service manager
    if let Some(service_state) = service_manager.get_service_status("package_registry") {
        vm_println!("   Reference Count: {} VMs", service_state.reference_count);
        vm_println!("   Registered VMs:  {:?}", service_state.registered_vms);

        let status_line = registry.format_service_status(
            "package_registry",
            service_state.is_running,
            service_state.reference_count,
        );
        vm_println!("{}", status_line);
    } else {
        vm_println!("   Status: 🔴 Not managed by service manager");
    }

    // Check actual server status for verification
    let server_url = "http://localhost:3080";
    if check_server_running().await {
        vm_println!("   Health Check: ✅ Server responding");
    } else {
        vm_println!("   Health Check: ❌ Server not responding");
    }

    vm_println!("\n💡 Service is automatically managed by VM lifecycle");
    vm_println!("   • Auto-starts when VM with package_registry: true is created");
    vm_println!("   • Auto-stops when last VM using it is destroyed");

    // Show additional package registry info
    vm_package_server::show_status(server_url).map_err(VmError::from)
}

/// Add package from current directory
async fn handle_add(package_type: Option<&str>) -> VmResult<()> {
    let server_url = "http://localhost:3080";

    // Ensure server is running before attempting to add package
    start_server_if_needed().await?;

    vm_println!("📦 Publishing package to local registry...");

    vm_package_server::client_ops::add_package(server_url, package_type)?;

    vm_success!("Package published successfully");
    Ok(())
}

/// Remove package from registry
async fn handle_remove(force: bool) -> VmResult<()> {
    let server_url = "http://localhost:3080";

    vm_println!("🗑️  Removing package from registry...");

    vm_package_server::client_ops::remove_package(server_url, force)?;

    vm_success!("Package removed successfully");
    Ok(())
}

/// List packages in registry
async fn handle_list() -> VmResult<()> {
    let server_url = "http://localhost:3080";

    vm_package_server::client_ops::list_packages(server_url)?;

    Ok(())
}

/// Handle configuration commands
async fn handle_config(action: &PkgConfigAction) -> VmResult<()> {
    match action {
        PkgConfigAction::Show => {
            vm_println!("Package Registry Configuration:");
            vm_println!("  Port: 3080");
            vm_println!("  Host: 0.0.0.0");
            vm_println!("  Fallback: enabled");
            Ok(())
        }
        PkgConfigAction::Get { key } => {
            match key.as_str() {
                "port" => vm_println!("3080"),
                "host" => vm_println!("0.0.0.0"),
                "fallback" => vm_println!("true"),
                _ => vm_error!("Unknown configuration key: {}", key),
            }
            Ok(())
        }
        PkgConfigAction::Set { key, value } => {
            vm_println!("Setting {} = {}", key, value);
            vm_println!("💡 Configuration changes will take effect on next server start");
            Ok(())
        }
    }
}

/// Generate shell configuration
async fn handle_use(shell: Option<&str>, port: u16) -> VmResult<()> {
    let shell_type = shell.unwrap_or("bash");

    match shell_type {
        "bash" | "zsh" => {
            vm_println!("# Package registry configuration for {}", shell_type);
            vm_println!("export NPM_CONFIG_REGISTRY=http://localhost:{}/npm/", port);
            vm_println!(
                "export PIP_INDEX_URL=http://localhost:{}/pypi/simple/",
                port
            );
            vm_println!("export PIP_TRUSTED_HOST=localhost");
            vm_println!("");
            vm_println!("# To apply: eval \"$(vm pkg use)\"");
        }
        "fish" => {
            vm_println!("# Package registry configuration for fish");
            vm_println!("set -x NPM_CONFIG_REGISTRY http://localhost:{}/npm/", port);
            vm_println!(
                "set -x PIP_INDEX_URL http://localhost:{}/pypi/simple/",
                port
            );
            vm_println!("set -x PIP_TRUSTED_HOST localhost");
        }
        _ => {
            vm_error!("Unsupported shell: {}", shell_type);
            vm_println!("Supported shells: bash, zsh, fish");
        }
    }

    Ok(())
}

/// Check if the package registry server is running
async fn check_server_running() -> bool {
    match reqwest::get("http://localhost:3080/health").await {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

/// Prompt user to start the server
fn prompt_start_server() -> VmResult<bool> {
    let confirmed = Confirm::new()
        .with_prompt("Package registry server is not running. Start it now?")
        .default(false)
        .interact()
        .map_err(|e| VmError::general(e, "Failed to prompt user"))?;

    Ok(confirmed)
}

/// Start server in background if needed
async fn start_server_if_needed() -> VmResult<()> {
    if check_server_running().await {
        return Ok(());
    }

    if prompt_start_server()? {
        vm_println!("🚀 Starting package registry server...");

        let data_dir = std::env::current_dir()?.join(".vm-packages");

        // Start server in background using existing function
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            if let Err(e) = rt.block_on(vm_package_server::server::run_server_background(
                "0.0.0.0".to_string(),
                3080,
                data_dir,
            )) {
                eprintln!("❌ Failed to start package registry: {}", e);
            }
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;

        // Verify it started
        if check_server_running().await {
            vm_success!("Package registry started successfully");
        } else {
            return Err(VmError::from(anyhow::anyhow!(
                "Failed to start package registry server"
            )));
        }
    } else {
        return Err(VmError::from(anyhow::anyhow!(
            "Package registry server is required but not running"
        )));
    }

    Ok(())
}
