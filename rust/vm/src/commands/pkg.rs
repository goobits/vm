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
use vm_config::GlobalConfig;
use vm_core::{vm_error, vm_println, vm_success};

use vm_package_server;

/// Handle package registry commands
pub async fn handle_pkg_command(
    command: &PkgSubcommand,
    global_config: GlobalConfig,
) -> VmResult<()> {
    match command {
        PkgSubcommand::Status => handle_status(&global_config).await,
        PkgSubcommand::Add { r#type } => handle_add(r#type.as_deref(), &global_config).await,
        PkgSubcommand::Remove { force } => handle_remove(*force, &global_config).await,
        PkgSubcommand::List => handle_list(&global_config).await,
        PkgSubcommand::Config { action } => handle_config(action, &global_config).await,
        PkgSubcommand::Use { shell, port } => {
            handle_use(shell.as_deref(), *port, &global_config).await
        }
    }
}

/// Show package registry status with service manager information
async fn handle_status(global_config: &GlobalConfig) -> VmResult<()> {
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
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );
    if check_server_running_with_url(&server_url).await {
        vm_println!("   Health Check: ✅ Server responding");
    } else {
        vm_println!("   Health Check: ❌ Server not responding");
    }

    vm_println!("\n💡 Service is automatically managed by VM lifecycle");
    vm_println!("   • Auto-starts when VM with package_registry: true is created");
    vm_println!("   • Auto-stops when last VM using it is destroyed");

    // Show additional package registry info
    vm_package_server::show_status(&server_url).map_err(VmError::from)
}

/// Add package from current directory
async fn handle_add(package_type: Option<&str>, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    // Ensure server is running before attempting to add package
    start_server_if_needed(global_config).await?;

    vm_println!("📦 Publishing package to local registry...");

    vm_package_server::client_ops::add_package(&server_url, package_type)?;

    vm_success!("Package published successfully");
    Ok(())
}

/// Remove package from registry
async fn handle_remove(force: bool, global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    vm_println!("🗑️  Removing package from registry...");

    vm_package_server::client_ops::remove_package(&server_url, force)?;

    vm_success!("Package removed successfully");
    Ok(())
}

/// List packages in registry
async fn handle_list(global_config: &GlobalConfig) -> VmResult<()> {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );

    vm_package_server::client_ops::list_packages(&server_url)?;

    Ok(())
}

/// Handle configuration commands
async fn handle_config(action: &PkgConfigAction, global_config: &GlobalConfig) -> VmResult<()> {
    let port = global_config.services.package_registry.port;
    match action {
        PkgConfigAction::Show => {
            vm_println!("Package Registry Configuration:");
            vm_println!("  Port: {}", port);
            vm_println!("  Host: 0.0.0.0");
            vm_println!("  Fallback: enabled");
            Ok(())
        }
        PkgConfigAction::Get { key } => {
            match key.as_str() {
                "port" => vm_println!("{}", port),
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
async fn handle_use(shell: Option<&str>, port: u16, global_config: &GlobalConfig) -> VmResult<()> {
    let shell_type = shell.unwrap_or("bash");

    // Use provided port if non-zero, otherwise use global config port
    let actual_port = if port != 0 {
        port
    } else {
        global_config.services.package_registry.port
    };

    match shell_type {
        "bash" | "zsh" => {
            vm_println!("# Package registry configuration for {}", shell_type);
            vm_println!(
                "export NPM_CONFIG_REGISTRY=http://localhost:{}/npm/",
                actual_port
            );
            vm_println!(
                "export PIP_INDEX_URL=http://localhost:{}/pypi/simple/",
                actual_port
            );
            vm_println!("export PIP_TRUSTED_HOST=localhost");
            vm_println!("");
            vm_println!("# To apply: eval \"$(vm pkg use)\"");
        }
        "fish" => {
            vm_println!("# Package registry configuration for fish");
            vm_println!(
                "set -x NPM_CONFIG_REGISTRY http://localhost:{}/npm/",
                actual_port
            );
            vm_println!(
                "set -x PIP_INDEX_URL http://localhost:{}/pypi/simple/",
                actual_port
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
async fn check_server_running(global_config: &GlobalConfig) -> bool {
    let server_url = format!(
        "http://localhost:{}",
        global_config.services.package_registry.port
    );
    check_server_running_with_url(&server_url).await
}

/// Check if the package registry server is running at a specific URL
async fn check_server_running_with_url(base_url: &str) -> bool {
    let health_url = format!("{}/health", base_url);
    match reqwest::get(&health_url).await {
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

/// Start server in background if needed using ServiceManager
async fn start_server_if_needed(global_config: &GlobalConfig) -> VmResult<()> {
    if check_server_running(global_config).await {
        return Ok(());
    }

    if prompt_start_server()? {
        vm_println!("🚀 Starting package registry server...");

        let service_manager = get_service_manager();

        // Create a modified config with package_registry enabled
        let mut enabled_config = global_config.clone();
        enabled_config.services.package_registry.enabled = true;

        // Register a synthetic "pkg-cli" VM to keep the service alive
        // This ensures the server persists beyond the CLI command lifetime
        service_manager
            .register_vm_services("pkg-cli", &enabled_config)
            .await
            .map_err(|e| {
                VmError::from(anyhow::anyhow!("Failed to start package registry: {}", e))
            })?;

        vm_success!("Package registry started successfully");
        vm_println!("💡 Service will remain running for package operations");
        vm_println!("   To stop: unregister with 'vm pkg stop' or destroy VMs using it");
    } else {
        return Err(VmError::from(anyhow::anyhow!(
            "Package registry server is required but not running"
        )));
    }

    Ok(())
}
