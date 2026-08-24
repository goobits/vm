//! VM destruction command handlers
//!
//! This module handles VM destruction including single instance destruction
//! and cross-provider bulk operations with pattern matching.

use dialoguer::{theme::ColorfulTheme, Select};
use std::io::IsTerminal;
use tracing::debug;

use crate::commands::db::utils::execute_psql_command;
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_hint, vm_println, vm_progress, vm_success, vm_warning};
use vm_provider::{InstanceProvider, Provider, ProviderContext, VmError as ProviderError};

use super::helpers::{has_enabled_services, unregister_vm_services_helper};
use super::target::canonical_instance_name;

/// Back up database services configured with `backup_on_destroy`.
///
/// Destruction must not begin until every requested backup succeeds.
async fn backup_databases(
    config: &VmConfig,
    vm_name: &str,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    use crate::commands::db::backup::backup_db;

    for (service_name, service_config) in &config.services {
        if service_config.backup_on_destroy != Some(true) {
            continue;
        }

        let db_name = format!("{}_{}", vm_name.replace('-', "_"), service_name);
        vm_progress!("Backing up database '{db_name}'...");

        backup_db(&db_name, None, global_config.backups.keep_count)
            .await
            .map_err(|error| {
                VmError::vm_operation(
                    error,
                    Some(vm_name),
                    format!("back up database '{db_name}' before destroy"),
                )
            })?;
        vm_success!("Backed up database '{db_name}'");
    }

    Ok(())
}

/// Handle VM destruction
pub async fn handle_destroy(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
    force: bool,
) -> VmResult<()> {
    // Get VM name from config for confirmation prompt
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("VM");

    let target_container = container
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| canonical_instance_name(provider.name(), vm_name, None));

    debug!(
        "Destroying VM: target_container='{}', provider='{}', force={}",
        target_container,
        provider.name(),
        force
    );
    let state = match provider.instance_state(container) {
        Ok(state) => state,
        Err(ProviderError::NotFound(_)) => {
            if has_enabled_services(&config, &global_config) {
                unregister_vm_services_helper(&target_container, &global_config).await?;
            }
            vm_success!("'{target_container}' is already removed");
            return Ok(());
        }
        Err(error) => return Err(VmError::from(error)),
    };

    let mut preserve_services = true;
    let should_destroy = if force {
        debug!("Force flag set - skipping confirmation prompt");
        vm_progress!("Removing '{target_container}'...");
        true
    } else {
        if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
            return Err(VmError::validation(
                "Removal requires confirmation in an interactive terminal",
                Some(format!(
                    "Review the target, then run `vm remove {target_container} --force`"
                )),
            ));
        }

        let service_manager_result = get_service_manager();
        let pg_service_check = if let Ok(sm) = service_manager_result {
            if let Some(pg_state) = sm.get_service_status("postgresql") {
                pg_state.is_running && pg_state.reference_count == 1
            } else {
                false
            }
        } else {
            false
        };

        if pg_service_check {
            let db_name = format!("{}_dev", vm_name.replace('-', "_"));
            let db_size = match execute_psql_command(&format!(
                "SELECT pg_size_pretty(pg_database_size('{db_name}'))"
            ))
            .await
            {
                Ok(size) => size.trim().to_string(),
                Err(_) => "N/A".to_string(),
            };

            vm_warning!("Managed PostgreSQL data will persist: {db_name} ({db_size})");
            vm_hint!("Back it up first with: vm db backup {db_name}");
        }

        let provider_name = provider_display_name(provider.as_ref());
        let resource_label = provider_resource_label(provider.as_ref());
        let destroyed_items = provider_destroyed_items(provider.as_ref());
        vm_println!("Remove {} environment '{}'?", provider_name, vm_name);
        vm_println!("  Provider:   {}", provider_name);
        vm_println!("  Status:     {}", state);
        vm_println!("  {}:  {}", resource_label, target_container);
        vm_warning!("This will permanently delete:");
        vm_println!("{}", destroyed_items);

        let options = &[
            "Destroy and preserve services",
            "Destroy and remove services",
            "Cancel",
        ];
        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose an option")
            .items(options)
            .default(0)
            .interact()
            .map_err(|e| VmError::general(e, "Failed to read user selection"))?;

        match selection {
            0 => {
                preserve_services = true;
                true
            }
            1 => {
                preserve_services = false;
                true
            }
            2 => false,
            _ => false,
        }
    };

    if !should_destroy {
        debug!("Destroy confirmation: response='no', cancelling destruction");
        vm_progress!("Removal cancelled");
        return Ok(());
    }

    debug!("Destroy confirmation: response='yes', proceeding with destruction");
    backup_databases(&config, vm_name, &global_config).await?;

    if !force {
        vm_progress!("Removing '{target_container}'...");
    }
    let context = ProviderContext::default().preserve_services(preserve_services);
    provider
        .destroy(container, &context)
        .map_err(VmError::from)?;

    if has_enabled_services(&config, &global_config) {
        unregister_vm_services_helper(&target_container, &global_config).await?;
    }
    vm_success!("Removed '{target_container}'");
    Ok(())
}

fn provider_display_name(provider: &dyn InstanceProvider) -> &'static str {
    match provider.name() {
        "docker" => "Docker",
        "podman" => "Podman",
        "tart" => "Tart",
        _ => "Provider",
    }
}

fn provider_resource_label(provider: &dyn InstanceProvider) -> &'static str {
    match provider.name() {
        "docker" | "podman" => "Container",
        "tart" => "VM",
        _ => "Resource",
    }
}

fn provider_destroyed_items(provider: &dyn InstanceProvider) -> &'static str {
    match provider.name() {
        "docker" | "podman" => {
            "  - Container and its writable layer\n\n  Managed named volumes are preserved."
        }
        "tart" => "  - Tart VM and all data",
        _ => "  - Provider resource and all data",
    }
}
