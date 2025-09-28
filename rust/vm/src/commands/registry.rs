//! Docker registry command handlers
//!
//! This module provides command handlers for the VM Docker registry functionality,
//! integrating with the vm-docker-registry library to provide Docker image caching
//! and pull-through proxy capabilities.

use crate::cli::RegistrySubcommand;
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use crate::service_registry::get_service_registry;
use dialoguer::Confirm;
use std::path::PathBuf;
use vm_common::{vm_println, vm_success};

use vm_docker_registry;

/// Handle Docker registry commands
pub async fn handle_registry_command(
    command: &RegistrySubcommand,
    _config: Option<PathBuf>,
) -> VmResult<()> {
    match command {
        RegistrySubcommand::Status => handle_status().await,
        RegistrySubcommand::Gc { force, dry_run } => handle_gc(*force, *dry_run).await,
        RegistrySubcommand::Config => handle_config().await,
        RegistrySubcommand::List => handle_list().await,
    }
}

/// Show Docker registry status with service manager information
async fn handle_status() -> VmResult<()> {
    let registry = get_service_registry();
    let service_manager = get_service_manager();

    vm_println!("📊 Docker Registry Status");

    // Get service status from service manager
    if let Some(service_state) = service_manager.get_service_status("docker_registry") {
        vm_println!("   Reference Count: {} VMs", service_state.reference_count);
        vm_println!("   Registered VMs:  {:?}", service_state.registered_vms);

        let status_line = registry.format_service_status(
            "docker_registry",
            service_state.is_running,
            service_state.reference_count,
        );
        vm_println!("{}", status_line);
    } else {
        vm_println!("   Status: 🔴 Not managed by service manager");
    }

    // Check actual registry status for verification
    if vm_docker_registry::check_registry_running(5000).await {
        vm_println!("   Health Check: ✅ Registry responding");
    } else {
        vm_println!("   Health Check: ❌ Registry not responding");
    }

    vm_println!("\n💡 Service is automatically managed by VM lifecycle");
    vm_println!("   • Auto-starts when VM with docker_registry: true is created");
    vm_println!("   • Auto-stops when last VM using it is destroyed");

    // Show additional registry info
    vm_docker_registry::status_registry()
        .await
        .map_err(VmError::from)
}

/// Run garbage collection
async fn handle_gc(force: bool, dry_run: bool) -> VmResult<()> {
    // Ensure registry is running before attempting garbage collection
    start_registry_if_needed().await?;

    Ok(vm_docker_registry::garbage_collect(force, dry_run).await?)
}

/// Show registry configuration
async fn handle_config() -> VmResult<()> {
    Ok(vm_docker_registry::show_config().await?)
}

/// List cached images
async fn handle_list() -> VmResult<()> {
    // Ensure registry is running before attempting to list images
    start_registry_if_needed().await?;

    Ok(vm_docker_registry::list_cached_images().await?)
}

/// Check if the Docker registry is running
async fn check_registry_running() -> bool {
    vm_docker_registry::check_registry_running(5000).await
}

/// Prompt user to start the registry
fn prompt_start_registry() -> VmResult<bool> {
    let confirmed = Confirm::new()
        .with_prompt("Docker registry is not running. Start it now?")
        .default(false)
        .interact()
        .map_err(|e| VmError::general(e, "Failed to prompt user"))?;

    Ok(confirmed)
}

/// Start registry if needed
async fn start_registry_if_needed() -> VmResult<()> {
    if check_registry_running().await {
        return Ok(());
    }

    if prompt_start_registry()? {
        vm_println!("🚀 Starting Docker registry...");

        vm_docker_registry::start_registry().await?;

        // Verify it started
        if check_registry_running().await {
            vm_success!("Docker registry started successfully");
        } else {
            return Err(VmError::from(anyhow::anyhow!(
                "Failed to start Docker registry"
            )));
        }
    } else {
        return Err(VmError::from(anyhow::anyhow!(
            "Docker registry is required but not running"
        )));
    }

    Ok(())
}
