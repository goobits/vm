//! VM destruction command handlers
//!
//! This module handles VM destruction including single instance destruction
//! and cross-provider bulk operations with pattern matching.

use dialoguer::{theme::ColorfulTheme, Select};
use tracing::debug;

use crate::commands::db::utils::execute_psql_command;
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::{Provider, ProviderContext};

use super::helpers::unregister_vm_services_helper;

/// Back up database services configured with `backup_on_destroy`.
///
/// Destruction must not begin until every requested backup succeeds. Callers can
/// explicitly bypass this gate with `--no-backup`.
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
        vm_println!("📦 Creating backup for database: {}", db_name);

        backup_db(&db_name, None, global_config.backups.keep_count)
            .await
            .map_err(|error| {
                VmError::vm_operation(
                    error,
                    Some(vm_name),
                    format!("back up database '{db_name}' before destroy"),
                )
            })?;
        vm_println!("✓ Backup created for {}", db_name);
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
    no_backup: bool,
    preserve_services: bool,
) -> VmResult<()> {
    // Get VM name from config for confirmation prompt
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("VM");

    let fallback_container_name = if provider.name() == "tart" {
        vm_name.to_string()
    } else {
        format!("{vm_name}-dev")
    };

    let target_container = provider
        .resolve_instance_name(container)
        .unwrap_or_else(|_| container.unwrap_or(&fallback_container_name).to_string());

    debug!(
        "Destroying VM: target_container='{}', provider='{}', force={}",
        target_container,
        provider.name(),
        force
    );
    // Check if the provider owns the target before showing confirmation.
    // This keeps Docker/Tart behavior aligned and avoids Docker-only probes.
    let container_exists = provider
        .list_instances()
        .map(|instances| {
            instances
                .iter()
                .any(|instance| instance.name == target_container)
        })
        .unwrap_or_else(|_| provider.status(container).is_ok());

    if !container_exists {
        vm_println!("{}", MESSAGES.vm.destroy_cleanup_already_removed);

        // Clean up Docker/Podman images even if the container is already gone.
        if let Some(executable) = container_runtime(provider.as_ref()) {
            let _ = std::process::Command::new(executable)
                .args(["image", "rm", "-f", &format!("{vm_name}-image")])
                .output();
        }

        unregister_vm_services_helper(&target_container, &global_config).await?;

        vm_println!("{}", MESSAGES.common.cleanup_complete);
        return Ok(());
    }

    let mut preserve_services = preserve_services;
    let should_destroy = if force {
        debug!("Force flag set - skipping confirmation prompt");
        vm_println!("{}", msg!(MESSAGES.vm.destroy_force, name = vm_name));
        true
    } else {
        // Check status to show current state
        let is_running = provider.status(container).is_ok();

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

            vm_println!("⚠️  Destroying VM '{}'", vm_name);
            vm_println!();
            vm_println!("📊 Database: Your PostgreSQL data will persist");
            vm_println!("   Location: ~/.vm/data/postgres");
            vm_println!("   Database: {} ({})", db_name, db_size);
            vm_println!();
            vm_println!("💡 Tip: Create a backup first");
            vm_println!("   vm db backup {}", db_name);
            vm_println!();
        }

        let provider_name = provider_display_name(provider.as_ref());
        let resource_label = provider_resource_label(provider.as_ref());
        let destroyed_items = provider_destroyed_items(provider.as_ref());
        let status = if is_running {
            MESSAGES.common.status_running
        } else {
            MESSAGES.common.status_stopped
        };

        vm_println!("🗑️ Destroy {} VM '{}'?\n", provider_name, vm_name);
        vm_println!("  Provider:   {}", provider_name);
        vm_println!("  Status:     {}", status);
        vm_println!("  {}:  {}", resource_label, target_container);
        vm_println!();
        vm_println!("⚠️  This will permanently delete:");
        vm_println!("{}", destroyed_items);
        vm_println!();

        let options = &[
            "Destroy and preserve services",
            "Destroy and remove services",
            "Cancel",
        ];
        let default_idx = if preserve_services { 0 } else { 1 };

        let selection = Select::with_theme(&ColorfulTheme::default())
            .with_prompt("Choose an option")
            .items(options)
            .default(default_idx)
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

    if should_destroy {
        debug!("Destroy confirmation: response='yes', proceeding with destruction");

        if !no_backup {
            vm_println!("🔄 Creating configured database backups...");
            backup_databases(&config, vm_name, &global_config).await?;
        }

        vm_println!("{}", MESSAGES.vm.destroy_progress);

        // Build context with preserve_services flag
        let context = ProviderContext::default().preserve_services(preserve_services);

        match provider.destroy_with_context(container, &context) {
            Ok(()) => {
                vm_println!("{}", MESSAGES.common.configuring_services);
                unregister_vm_services_helper(&target_container, &global_config).await?;

                vm_println!("{}", MESSAGES.vm.destroy_success);
                Ok(())
            }
            Err(e) => {
                vm_println!("\n❌ Destruction failed: {}", e);
                Err(VmError::from(e))
            }
        }
    } else {
        debug!("Destroy confirmation: response='no', cancelling destruction");
        vm_println!("{}", MESSAGES.vm.destroy_cancelled);
        vm_error!("VM destruction cancelled by user");
        Err(VmError::general(
            std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "VM destruction cancelled by user",
            ),
            "User cancelled VM destruction",
        ))
    }
}

fn container_runtime(provider: &dyn Provider) -> Option<&str> {
    match provider.name() {
        "docker" => Some("docker"),
        "podman" => Some("podman"),
        _ => None,
    }
}

fn provider_display_name(provider: &dyn Provider) -> &'static str {
    match provider.name() {
        "docker" => "Docker",
        "podman" => "Podman",
        "tart" => "Tart",
        _ => "Provider",
    }
}

fn provider_resource_label(provider: &dyn Provider) -> &'static str {
    match provider.name() {
        "docker" | "podman" => "Container",
        "tart" => "VM",
        _ => "Resource",
    }
}

fn provider_destroyed_items(provider: &dyn Provider) -> &'static str {
    match provider.name() {
        "docker" | "podman" => "  • Container and all data\n  • Docker image and build cache",
        "tart" => "  • Tart VM and all data",
        _ => "  • Provider resource and all data",
    }
}
