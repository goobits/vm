//! VM destruction command handlers
//!
//! This module handles VM destruction including single instance destruction
//! and cross-provider bulk operations with pattern matching.

use std::io::{self, Write};

use tracing::{debug, info_span};

use crate::commands::db::utils::execute_psql_command;
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::{InstanceInfo, Provider};

use super::helpers::unregister_vm_services_helper;
use super::list::{get_all_instances, get_instances_from_provider};

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
) -> VmResult<()> {
    // Get VM name from config for confirmation prompt
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("VM");

    let config_container_name = format!("{vm_name}-dev");

    // Use provided container name if given, otherwise use config-derived name
    let target_container = if let Some(container_name) = container {
        container_name.to_string()
    } else {
        config_container_name.clone()
    };

    debug!(
        "Destroying VM: target_container='{}', provider='{}', force={}",
        target_container,
        provider.name(),
        force
    );

    // Check if container exists before showing confirmation
    let container_exists = std::process::Command::new("docker")
        .args(["inspect", &target_container])
        .output()
        .ok()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !container_exists {
        vm_println!("{}", MESSAGES.vm.destroy_cleanup_already_removed);

        // Clean up images even if container doesn't exist
        let _ = std::process::Command::new("docker")
            .args(["image", "rm", "-f", &format!("{vm_name}-image")])
            .output();

        unregister_vm_services_helper(&target_container, &global_config).await?;

        vm_println!("{}", MESSAGES.common.cleanup_complete);
        return Ok(());
    }

    let should_destroy = if force {
        debug!("Force flag set - skipping confirmation prompt");
        vm_println!("{}", msg!(MESSAGES.vm.destroy_force, name = vm_name));
        true
    } else {
        // Check status to show current state
        let is_running = provider.status(container).is_ok();

        let service_manager = get_service_manager();
        if let Some(pg_state) = service_manager.get_service_status("postgresql") {
            if pg_state.is_running && pg_state.reference_count == 1 {
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
        }

        vm_println!("{}", msg!(MESSAGES.vm.destroy_confirm, name = vm_name));
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm.destroy_info_block,
                status = if is_running {
                    MESSAGES.common.status_running
                } else {
                    MESSAGES.common.status_stopped
                },
                container = &target_container
            )
        );
        print!("{}", MESSAGES.vm.destroy_confirm_prompt);
        io::stdout()
            .flush()
            .map_err(|e| VmError::general(e, "Failed to flush stdout"))?;

        let mut response = String::new();
        io::stdin()
            .read_line(&mut response)
            .map_err(|e| VmError::general(e, "Failed to read user input"))?;
        response.trim().to_lowercase() == "y"
    };

    if should_destroy {
        debug!("Destroy confirmation: response='yes', proceeding with destruction");
        vm_println!("{}", MESSAGES.vm.destroy_progress);

        match provider.destroy(container) {
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
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "destroy");
    let _enter = span.enter();

    if *all || provider_filter.is_some() || pattern.is_some() {
        // Cross-provider destroy operations
        return handle_cross_provider_destroy(*all, provider_filter, pattern, *force);
    }

    // Single instance destroy (existing behavior)
    handle_destroy(
        provider,
        container,
        config,
        global_config,
        *force,
        *no_backup,
    )
    .await
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

/// Simple pattern matching for instance names
fn match_pattern(name: &str, pattern: &str) -> bool {
    if pattern.contains('*') {
        // Simple wildcard matching
        if pattern == "*" {
            true
        } else if pattern.starts_with('*') && pattern.ends_with('*') {
            let middle = &pattern[1..pattern.len() - 1];
            name.contains(middle)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            name.ends_with(suffix)
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            name.starts_with(prefix)
        } else {
            // Pattern has * in the middle - basic implementation
            name == pattern
        }
    } else {
        name == pattern
    }
}
