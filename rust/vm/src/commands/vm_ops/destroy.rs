//! Verified single-environment removal and its confirmation contract.

use tracing::debug;

use crate::commands::db::route::DbRoute;
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_println, vm_progress, vm_success};
use vm_provider::{Provider, ProviderContext, VmError as ProviderError};

use super::data_ownership::{plan_data_deletion, remove_exclusive_volumes};
use super::helpers::{has_enabled_services, unregister_vm_services_helper};
use super::target::canonical_instance_name;
use super::target::verify_runtime_owner;

/// Back up database services configured with `backup_on_destroy`.
///
/// Destruction must not begin until every requested backup succeeds.
async fn backup_databases(
    config: &VmConfig,
    target: &str,
    provider_name: &str,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    use crate::commands::db::backup::backup_db;

    for (service_name, service_config) in &config.services {
        if service_config.backup_on_destroy != Some(true) {
            continue;
        }

        if service_name != "postgresql" {
            return Err(VmError::validation(
                format!("Backup before removal is not supported for service '{service_name}'"),
                None::<String>,
            ));
        }
        let environment = config.environments.keys().find(|name| {
            super::target::canonical_instance_name(
                provider_name,
                config
                    .project
                    .as_ref()
                    .and_then(|project| project.name.as_deref())
                    .unwrap_or("vm-project"),
                Some(name),
            ) == target
        });
        let route = DbRoute::for_config(
            config,
            environment.map(String::as_str),
            global_config.container_provider().as_str(),
        )?;
        let db_name = &route.database;
        vm_progress!("Backing up database '{db_name}'...");

        backup_db(&route, db_name, None, global_config.backups.keep_count)
            .await
            .map_err(|error| {
                VmError::vm_operation(
                    error,
                    Some(target),
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
    delete_data: bool,
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

    debug!(target = %target_container, provider = provider.name(), delete_data, "Removing environment");
    if provider.name() == "tart" && !delete_data {
        return Err(VmError::validation(
            "Tart stores persistent data inside the VM disk, which its delete operation removes",
            Some("Use `vm remove --delete-data` after saving any data you need"),
        ));
    }
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

    debug!(%state, "Verified environment state");
    verify_runtime_owner(provider.as_ref(), &config, &target_container)?;
    let data_plan = if delete_data && matches!(provider.name(), "docker" | "podman") {
        Some(plan_data_deletion(
            provider.name(),
            &target_container,
            &config,
        )?)
    } else {
        None
    };
    backup_databases(&config, &target_container, provider.name(), &global_config).await?;
    verify_runtime_owner(provider.as_ref(), &config, &target_container)?;
    vm_progress!("Removing '{target_container}'...");
    let context = ProviderContext::default().preserve_services(false);
    provider
        .destroy(container, &context)
        .map_err(VmError::from)?;

    if has_enabled_services(&config, &global_config) {
        unregister_vm_services_helper(&target_container, &global_config).await?;
    }
    if let Some(plan) = data_plan {
        remove_exclusive_volumes(&plan)?;
    }
    vm_success!("Removed '{target_container}' (declaration remains; `vm start` can recreate it)");
    Ok(())
}

pub fn confirm_removal(targets: &[String], delete_data: bool, yes: bool) -> VmResult<bool> {
    if targets.is_empty() {
        return Err(VmError::validation(
            "No environments selected for removal",
            None::<String>,
        ));
    }
    vm_println!("Remove {} environment(s):", targets.len());
    for target in targets {
        vm_println!("  {target}");
    }
    let data_policy = if delete_data {
        "Exclusively owned persistent data will also be deleted."
    } else {
        "Persistent data and snapshots will be preserved."
    };
    vm_println!("{data_policy}");
    crate::confirmation::destructive("Proceed with removal?", yes)
}
