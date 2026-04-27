//! VM destruction command handlers
//!
//! This module handles VM destruction including single instance destruction
//! and cross-provider bulk operations with pattern matching.

use std::io::{self, Write};

use dialoguer::{theme::ColorfulTheme, Select};
use tracing::{debug, info_span};

use crate::commands::db::utils::execute_psql_command;
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::{get_provider, InstanceInfo, Provider, ProviderContext};

use super::helpers::unregister_vm_services_helper;
use super::targets::{get_all_instances, get_instances_from_provider, match_pattern};

/// Helper function to backup database services configured with backup_on_destroy
async fn backup_databases(config: &VmConfig, vm_name: &str, global_config: &GlobalConfig) {
    use crate::commands::db::backup::backup_db;

    for (service_name, service_config) in &config.services {
        if service_config.backup_on_destroy != Some(true) {
            continue;
        }

        let db_name = format!("{}_{}", vm_name.replace('-', "_"), service_name);
        vm_println!("📦 Creating backup for database: {}", db_name);

        if let Err(e) = backup_db(&db_name, None, global_config.backups.keep_count).await {
            vm_println!("⚠️  Warning: Failed to backup {}: {}", db_name, e);
        } else {
            vm_println!("✓ Backup created for {}", db_name);
        }
    }
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
        vm_println!("{}", MESSAGES.vm.destroy_progress);

        // Build context with preserve_services flag
        let context = ProviderContext::default().preserve_services(preserve_services);

        match provider.destroy_with_context(container, &context) {
            Ok(()) => {
                // Backup database services if configured (run in background)
                if !no_backup {
                    // Clone values for the background task
                    let config_clone = config.clone();
                    let vm_name_clone = vm_name.to_string();
                    let global_config_clone = global_config.clone();

                    tokio::spawn(async move {
                        vm_println!("🔄 Starting database backups in background...");
                        backup_databases(&config_clone, &vm_name_clone, &global_config_clone).await;
                        vm_println!("✅ Database backups completed");
                    });

                    vm_println!("⏩ VM destroy continuing (backups running in background)...");
                }

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

/// Enhanced destroy handler with cross-provider support
#[allow(clippy::too_many_arguments)]
pub async fn handle_destroy_enhanced(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
    force: &bool,
    no_backup: &bool,
    all: &bool,
    provider_filter: Option<&str>,
    pattern: Option<&str>,
    preserve_services: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "destroy");
    let _enter = span.enter();

    if *all || pattern.is_some() {
        // Cross-provider destroy operations
        return handle_cross_provider_destroy(*all, provider_filter, pattern, *force);
    }

    let (provider, container) = resolve_single_destroy_target(provider, container, &config);

    // Single instance destroy
    handle_destroy(
        provider,
        container.as_deref(),
        config,
        global_config,
        *force,
        *no_backup,
        preserve_services,
    )
    .await
}

fn is_provider_name(value: &str) -> bool {
    matches!(value, "docker" | "podman" | "tart")
}

fn provider_for_name(provider_name: &str, config: &VmConfig) -> VmResult<Box<dyn Provider>> {
    let mut provider_config = config.clone();
    provider_config.provider = Some(provider_name.to_string());
    get_provider(provider_config).map_err(VmError::from)
}

fn project_instance_matches(instance: &InstanceInfo, project_name: &str) -> bool {
    instance.project.as_deref() == Some(project_name)
        || instance.name == project_name
        || instance.name == format!("{project_name}-dev")
}

fn resolve_project_provider(current_provider: &str, config: &VmConfig) -> Option<String> {
    let project_name = config.project.as_ref().and_then(|p| p.name.as_deref())?;

    let instances = get_all_instances().ok()?;
    let matches: Vec<_> = instances
        .into_iter()
        .filter(|instance| project_instance_matches(instance, project_name))
        .collect();

    matches
        .iter()
        .find(|instance| instance.provider == current_provider)
        .or_else(|| {
            matches
                .iter()
                .find(|instance| instance.status.to_lowercase().contains("running"))
        })
        .or_else(|| matches.first())
        .map(|instance| instance.provider.clone())
}

fn resolve_single_destroy_target(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: &VmConfig,
) -> (Box<dyn Provider>, Option<String>) {
    if let Some(provider_name) = container.filter(|value| is_provider_name(value)) {
        if let Ok(provider) = provider_for_name(provider_name, config) {
            return (provider, None);
        }
    }

    if container.is_none() {
        if let Some(provider_name) = resolve_project_provider(provider.name(), config) {
            if provider_name != provider.name() {
                if let Ok(provider) = provider_for_name(&provider_name, config) {
                    return (provider, None);
                }
            }
        }
    }

    (provider, container.map(ToString::to_string))
}

/// Handle destroying instances across providers
fn handle_cross_provider_destroy(
    all: bool,
    provider_filter: Option<&str>,
    pattern: Option<&str>,
    force: bool,
) -> VmResult<()> {
    debug!(
        "Cross-provider destroy: all={}, provider_filter={:?}, pattern={:?}, force={}",
        all, provider_filter, pattern, force
    );

    // Get all instances to destroy
    let instances_to_destroy = if let Some(provider_name) = provider_filter {
        get_instances_from_provider(provider_name)?
    } else {
        get_all_instances()?
    };

    // Filter by pattern if provided
    let filtered_instances: Vec<_> = if let Some(pattern_str) = pattern {
        instances_to_destroy
            .into_iter()
            .filter(|instance| match_pattern(&instance.name, pattern_str))
            .collect()
    } else {
        instances_to_destroy
    };

    if filtered_instances.is_empty() {
        vm_println!("{}", MESSAGES.vm.destroy_cross_no_instances);
        return Ok(());
    }

    // Show what will be destroyed
    vm_println!("{}", MESSAGES.vm.destroy_cross_list_header);
    for instance in &filtered_instances {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm.destroy_cross_list_item,
                name = &instance.name,
                provider = &instance.provider
            )
        );
    }

    let should_destroy = if force {
        true
    } else {
        print!(
            "{}",
            msg!(
                MESSAGES.vm.destroy_cross_confirm_prompt,
                count = filtered_instances.len().to_string()
            )
        );
        io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    };

    if !should_destroy {
        vm_println!("{}", MESSAGES.vm.destroy_cross_cancelled);
        return Ok(());
    }

    // Destroy each instance
    let mut success_count = 0;
    let mut error_count = 0;

    for instance in filtered_instances {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm.destroy_cross_progress,
                name = &instance.name,
                provider = &instance.provider
            )
        );

        let result = destroy_single_instance(&instance);
        match result {
            Ok(()) => {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.vm.destroy_cross_success_item,
                        name = &instance.name
                    )
                );
                success_count += 1;
            }
            Err(e) => {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.vm.destroy_cross_failed,
                        name = &instance.name,
                        error = e.to_string()
                    )
                );
                error_count += 1;
            }
        }
    }

    vm_println!(
        "{}",
        msg!(
            MESSAGES.vm.destroy_cross_complete,
            success = success_count.to_string(),
            errors = error_count.to_string()
        )
    );

    Ok(())
}

/// Destroy a single instance using its provider
fn destroy_single_instance(instance: &InstanceInfo) -> VmResult<()> {
    use vm_config::config::VmConfig;
    use vm_provider::get_provider;

    let config = VmConfig {
        provider: Some(instance.provider.clone()),
        ..Default::default()
    };

    let provider = get_provider(config)?;
    Ok(provider.destroy(Some(&instance.name))?)
}
