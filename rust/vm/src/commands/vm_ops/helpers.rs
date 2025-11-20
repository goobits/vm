//! Helper functions shared across VM operations
//!
//! This module provides utilities and service management functions
//! used by multiple VM command handlers.

use tracing::{debug, warn};

use crate::error::VmResult;
use crate::service_manager::get_service_manager;
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_provider::Provider;

/// Handle get sync directory
pub fn handle_get_sync_directory(provider: Box<dyn Provider>) {
    debug!("Getting sync directory for provider '{}'", provider.name());
    let sync_dir = provider.get_sync_directory();
    debug!("Sync directory: '{}'", sync_dir);
    println!("{sync_dir}");
}

/// Helper function to register VM services
pub(super) async fn register_vm_services_helper(
    vm_name: &str,
    vm_config: &VmConfig,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let service_manager = match get_service_manager() {
        Ok(sm) => sm,
        Err(e) => {
            warn!("Failed to get service manager: {}", e);
            return Ok(());
        }
    };

    if let Err(e) = service_manager
        .register_vm_services(vm_name, vm_config, global_config)
        .await
    {
        warn!("Failed to register VM services: {}", e);
        vm_println!(
            "{}",
            msg!(
                MESSAGES.common.services_config_failed,
                error = e.to_string()
            )
        );
        // Don't fail the operation if service registration fails
    } else {
        vm_println!("{}", MESSAGES.common.services_config_success);
    }
    Ok(())
}

/// Helper function to unregister VM services
pub(super) async fn unregister_vm_services_helper(
    vm_name: &str,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let service_manager = match get_service_manager() {
        Ok(sm) => sm,
        Err(e) => {
            warn!("Failed to get service manager: {}", e);
            return Ok(());
        }
    };

    if let Err(e) = service_manager
        .unregister_vm_services(vm_name, global_config)
        .await
    {
        warn!("Failed to unregister VM services: {}", e);
        vm_println!(
            "{}",
            msg!(
                MESSAGES.common.services_cleanup_failed,
                error = e.to_string()
            )
        );
        // Don't fail the operation if service cleanup fails
    } else {
        vm_println!("{}", MESSAGES.common.services_cleaned);
    }
    Ok(())
}
