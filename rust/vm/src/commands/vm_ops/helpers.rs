//! Helper functions shared across VM operations
//!
//! This module provides utilities and service management functions
//! used by multiple VM command handlers.

use tracing::warn;

use crate::error::VmResult;
use crate::services::service_lifecycle;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::msg;
use vm_core::{vm_println, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

pub(super) fn has_enabled_services(config: &VmConfig, global: &GlobalConfig) -> bool {
    config.services.values().any(|service| service.enabled)
        || global.services.auth_proxy.enabled
        || global.services.postgresql.enabled
        || global.services.redis.enabled
        || global.services.mongodb.enabled
        || global.services.mysql.enabled
}

/// Helper function to register VM services
pub(super) async fn register_vm_services_helper(
    vm_name: &str,
    vm_config: &VmConfig,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let lifecycle = match service_lifecycle() {
        Ok(lifecycle) => lifecycle,
        Err(e) => {
            warn!("Failed to get service lifecycle: {}", e);
            vm_warning!("Service lifecycle unavailable: {e}");
            return Ok(());
        }
    };

    if let Err(e) = lifecycle
        .register_vm_services(vm_name, vm_config, global_config)
        .await
    {
        warn!("Failed to register VM services: {}", e);
        vm_warning!("Service configuration failed: {e}");
        // Don't fail the operation if service registration fails
    } else {
        vm_success!("Services configured");
    }
    Ok(())
}

/// Helper function to unregister VM services
pub(super) async fn unregister_vm_services_helper(
    vm_name: &str,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let lifecycle = match service_lifecycle() {
        Ok(lifecycle) => lifecycle,
        Err(e) => {
            warn!("Failed to get service lifecycle: {}", e);
            vm_warning!("Service lifecycle unavailable: {e}");
            return Ok(());
        }
    };

    if let Err(e) = lifecycle
        .unregister_vm_services(vm_name, global_config)
        .await
    {
        warn!("Failed to unregister VM services: {}", e);
        vm_warning!("Service cleanup failed: {e}");
        // Don't fail the operation if service cleanup fails
    } else {
        vm_success!("Services cleaned up");
    }
    Ok(())
}

/// Print VM resource/services/ports details in a consistent format.
pub(super) fn print_vm_runtime_details(config: &VmConfig, include_ports: bool) {
    // Show resources if available
    if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus.as_ref()) {
        if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
            let cpu_str = match cpus.to_count() {
                Some(count) => count.to_string(),
                None => "unlimited".to_string(),
            };
            let mem_str = match memory.to_mb() {
                Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                Some(mb) => format!("{mb}MB"),
                None => "unlimited".to_string(),
            };
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.common.resources_label,
                    cpus = cpu_str,
                    memory = mem_str
                )
            );
        }
    }

    // Show services if any are configured
    let services: Vec<String> = config
        .services
        .iter()
        .filter(|(_, svc)| svc.enabled)
        .map(|(name, _)| name.clone())
        .collect();

    if !services.is_empty() {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.common.services_label,
                services = services.join(", ")
            )
        );
    }

    if include_ports {
        if let Some(range) = &config.ports.range {
            if range.len() == 2 {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.common.ports_label,
                        start = range[0].to_string(),
                        end = range[1].to_string()
                    )
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::has_enabled_services;
    use vm_config::{config::VmConfig, GlobalConfig};

    #[test]
    fn enabled_service_guard_includes_global_configuration() {
        let config = VmConfig::default();
        let mut global = GlobalConfig::default();

        assert!(!has_enabled_services(&config, &global));
        global.services.postgresql.enabled = true;
        assert!(has_enabled_services(&config, &global));
    }
}
