// VM operation command handlers
// Enhanced with multi-instance support and proper tooling

// Standard library
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

// External crates
use tracing::{debug, info, info_span, warn};

// Internal imports
use crate::error::{VmError, VmResult};
use crate::service_manager::get_service_manager;
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::{InstanceInfo, Provider, ProviderContext, VmStatusReport};

/// Handle VM creation
pub async fn handle_create(
    provider: Box<dyn Provider>,
    config: VmConfig,
    global_config: GlobalConfig,
    force: bool,
    instance: Option<String>,
    verbose: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "create");
    let _enter = span.enter();
    info!("Starting VM creation");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    // Check if this is a multi-instance provider and handle accordingly
    if provider.supports_multi_instance() && instance.is_some() {
        let instance_name = instance.as_deref().unwrap();
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_create_header_instance,
                instance = instance_name,
                name = vm_name
            )
        );

        if force {
            debug!("Force flag set - will destroy existing instance if present");
            // Try to destroy specific instance first
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_create_force_recreating_instance,
                    name = instance_name
                )
            );
            if let Err(e) = provider.destroy(Some(instance_name)) {
                warn!(
                    "Failed to destroy existing instance '{}' during force create: {}",
                    instance_name, e
                );
                // Continue with creation even if destroy fails
            }
        }
    } else {
        // Standard single-instance creation
        if let Some(instance_name) = &instance {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_create_multiinstance_warning,
                    instance = instance_name,
                    provider = provider.name()
                )
            );
        }

        if force {
            debug!("Force flag set - will destroy existing VM if present");
            // Check if VM exists and destroy it first
            if provider.status(None).is_ok() {
                warn!("VM exists, destroying due to --force flag");
                vm_println!(
                    "{}",
                    msg!(MESSAGES.vm_create_force_recreating, name = vm_name)
                );
                provider.destroy(None).map_err(VmError::from)?;
            }
        }
    }

    vm_println!("{}", msg!(MESSAGES.vm_create_header, name = vm_name));
    vm_println!("{}", MESSAGES.vm_create_progress);

    // Create provider context with verbose flag and global config
    let context = ProviderContext::with_verbose(verbose).with_config(global_config.clone());

    // Call the appropriate create method based on whether instance is specified
    let create_result = if let Some(instance_name) = &instance {
        if provider.supports_multi_instance() {
            provider.create_instance_with_context(instance_name, &context)
        } else {
            provider.create_with_context(&context)
        }
    } else {
        provider.create_with_context(&context)
    };

    match create_result {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm_create_success);

            let container_name = if let Some(instance_name) = &instance {
                format!("{}-{}", vm_name, instance_name)
            } else {
                format!("{}-dev", vm_name)
            };
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_create_info_block,
                    status = MESSAGES.common_status_running,
                    container = container_name
                )
            );

            // Show resources if available
            if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus) {
                if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
                    // Format memory display
                    let mem_str = match memory.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{}MB", mb),
                        None => format!("{:?}", memory),
                    };
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.common_resources_label,
                            cpus = cpus.to_string(),
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
                        MESSAGES.common_services_label,
                        services = services.join(", ")
                    )
                );
            }

            // Show port range
            if let Some(range) = &config.ports.range {
                if range.len() == 2 {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.common_ports_label,
                            start = range[0].to_string(),
                            end = range[1].to_string()
                        )
                    );
                }
            }

            // Register VM services (VM is already created and started by provider.create())
            let vm_instance_name = if let Some(instance_name) = &instance {
                format!("{}-{}", vm_name, instance_name)
            } else {
                format!("{}-dev", vm_name)
            };

            vm_println!("{}", MESSAGES.common_configuring_services);
            register_vm_services_helper(&vm_instance_name, &global_config).await?;

            vm_println!("{}", MESSAGES.common_connect_hint);
            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_create_troubleshooting,
                    name = vm_name,
                    error = e.to_string()
                )
            );
            Err(VmError::from(e))
        }
    }
}

/// Handle VM start
pub async fn handle_start(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "start");
    let _enter = span.enter();
    info!("Starting VM");

    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    let container_name = format!("{}-dev", vm_name);

    // Check if container exists and is running
    // We need to check using Docker directly since provider.status() just shows status
    let container_exists = std::process::Command::new("docker")
        .args(["ps", "-a", "--format", "{{.Names}}"])
        .output()
        .ok()
        .and_then(|output| {
            let names = String::from_utf8_lossy(&output.stdout);
            if names.lines().any(|name| name.trim() == container_name) {
                Some(())
            } else {
                None
            }
        })
        .is_some();

    if container_exists {
        // Check if it's actually running
        let is_running = std::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Status}}", &container_name])
            .output()
            .ok()
            .and_then(|output| {
                let status = String::from_utf8_lossy(&output.stdout);
                if status.trim() == "running" {
                    Some(())
                } else {
                    None
                }
            })
            .is_some();

        if is_running {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_start_already_running, name = vm_name)
            );
            return Ok(());
        }
    }

    vm_println!("{}", msg!(MESSAGES.vm_start_header, name = vm_name));

    let context = ProviderContext::with_verbose(false).with_config(global_config.clone());
    match provider.start_with_context(container, &context) {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm_start_success);

            // Show VM details
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_start_info_block,
                    status = MESSAGES.common_status_running,
                    container = container_name
                )
            );

            // Show resources if available
            if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus) {
                if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
                    // Format memory display
                    let mem_str = match memory.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{}MB", mb),
                        None => format!("{:?}", memory),
                    };
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.common_resources_label,
                            cpus = cpus.to_string(),
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
                        MESSAGES.common_services_label,
                        services = services.join(", ")
                    )
                );
            }

            // Register VM services and auto-start them
            let vm_instance_name = format!("{}-dev", vm_name);

            vm_println!("{}", MESSAGES.common_configuring_services);
            register_vm_services_helper(&vm_instance_name, &global_config).await?;

            vm_println!("{}", MESSAGES.common_connect_hint);

            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_start_troubleshooting,
                    name = vm_name,
                    error = e.to_string(),
                    container = container_name
                )
            );
            Err(VmError::from(e))
        }
    }
}

/// Handle VM stop - graceful stop for current project or force kill specific container
pub async fn handle_stop(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    match container {
        None => {
            // Graceful stop of current project VM
            let span = info_span!("vm_operation", operation = "stop");
            let _enter = span.enter();
            info!("Stopping VM");

            let vm_name = config
                .project
                .as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("vm-project");

            vm_println!("{}", msg!(MESSAGES.vm_stop_header, name = vm_name));

            match provider.stop(None) {
                Ok(()) => {
                    // Unregister VM services after successful stop
                    let vm_instance_name = format!("{}-dev", vm_name);

                    vm_println!("{}", MESSAGES.vm_stop_success);
                    unregister_vm_services_helper(&vm_instance_name).await?;

                    vm_println!("{}", MESSAGES.vm_stop_restart_hint);
                    Ok(())
                }
                Err(e) => {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.vm_stop_troubleshooting,
                            name = vm_name,
                            error = e.to_string()
                        )
                    );
                    Err(VmError::from(e))
                }
            }
        }
        Some(container_name) => {
            // Force kill specific container
            let span = info_span!("vm_operation", operation = "kill");
            let _enter = span.enter();
            warn!("Force killing container: {}", container_name);

            vm_println!(
                "{}",
                msg!(MESSAGES.vm_stop_force_header, name = container_name)
            );

            match provider.kill(Some(container_name)) {
                Ok(()) => {
                    // For force kill, still unregister services for cleanup
                    vm_println!("{}", MESSAGES.vm_stop_force_success);
                    unregister_vm_services_helper(container_name).await?;
                    Ok(())
                }
                Err(e) => {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.vm_stop_force_troubleshooting,
                            error = e.to_string()
                        )
                    );
                    Err(VmError::from(e))
                }
            }
        }
    }
}

/// Handle VM restart
pub async fn handle_restart(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "restart");
    let _enter = span.enter();
    info!("Restarting VM");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    vm_println!("{}", msg!(MESSAGES.vm_restart_header, name = vm_name));

    // Use provider.restart_with_context() for the actual VM restart, then handle services
    let context = ProviderContext::with_verbose(false).with_config(global_config.clone());
    match provider.restart_with_context(container, &context) {
        Ok(()) => {
            // After successful restart, register services
            let vm_instance_name = format!("{}-dev", vm_name);
            vm_println!("{}", MESSAGES.common_configuring_services);
            register_vm_services_helper(&vm_instance_name, &global_config).await?;
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.vm_restart_troubleshooting,
                    name = vm_name,
                    error = e.to_string()
                )
            );
            return Err(VmError::from(e));
        }
    }

    vm_println!("{}", MESSAGES.vm_restart_success);
    Ok(())
}

/// Handle VM provisioning
pub fn handle_provision(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "provision");
    let _enter = span.enter();
    info!("Re-running VM provisioning");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    vm_println!("{}", msg!(MESSAGES.vm_provision_header, name = vm_name));
    vm_println!("{}", MESSAGES.vm_provision_progress);

    match provider.provision(container) {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm_provision_success);
            vm_println!("{}", MESSAGES.vm_provision_hint);
            Ok(())
        }
        Err(e) => {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_provision_troubleshooting, error = e.to_string())
            );
            Err(VmError::from(e))
        }
    }
}

/// Handle VM listing with enhanced filtering options
pub fn handle_list_enhanced(
    _provider: Box<dyn Provider>,
    _all_providers: &bool,
    provider_filter: Option<&str>,
    _verbose: &bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "list");
    let _enter = span.enter();
    debug!(
        "Listing VMs with enhanced filtering - provider_filter: {:?}",
        provider_filter
    );

    // Get all instances from all providers (or filtered)
    let all_instances = if let Some(provider_name) = provider_filter {
        get_instances_from_provider(provider_name)?
    } else {
        get_all_instances()?
    };

    if all_instances.is_empty() {
        if let Some(provider_name) = provider_filter {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_list_empty_provider, provider = provider_name)
            );
        } else {
            vm_println!("{}", MESSAGES.vm_list_empty);
        }
        return Ok(());
    }

    // Rich dashboard table (always displayed)
    vm_println!("{}", MESSAGES.vm_list_table_header);
    vm_println!("{}", MESSAGES.vm_list_table_separator);

    // Sort instances by provider then name for consistent output
    let mut sorted_instances = all_instances;
    sorted_instances.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));

    for instance in sorted_instances {
        println!(
            "{:<20} {:<10} {:<12} {:<20} {:<10} {:<15}",
            truncate_string(&instance.name, 20),
            instance.provider,
            format_status(&instance.status),
            truncate_string(&instance.id, 20),
            format_uptime(&instance.uptime),
            instance.project.as_deref().unwrap_or("--")
        );
    }

    Ok(())
}

/// Legacy handle_list for backward compatibility
#[allow(dead_code)]
pub fn handle_list(provider: Box<dyn Provider>) -> VmResult<()> {
    handle_list_enhanced(provider, &true, None, &false)
}

// Helper function to get instances from all available providers
fn get_all_instances() -> VmResult<Vec<InstanceInfo>> {
    use vm_config::config::VmConfig;
    use vm_provider::get_provider;

    let mut all_instances = Vec::new();
    let providers = ["docker", "tart", "vagrant"];

    for provider_name in providers {
        // Try to create each provider
        let config = VmConfig {
            provider: Some(provider_name.to_string()),
            ..Default::default()
        };

        match get_provider(config) {
            Ok(provider) => {
                // Get instances from this provider
                match provider.list_instances() {
                    Ok(instances) => {
                        debug!(
                            "Found {} instances from {} provider",
                            instances.len(),
                            provider_name
                        );
                        all_instances.extend(instances);
                    }
                    Err(e) => {
                        debug!(
                            "Failed to list instances from {} provider: {}",
                            provider_name, e
                        );
                        // Continue with other providers
                    }
                }
            }
            Err(e) => {
                debug!("Provider {} not available: {}", provider_name, e);
                // Continue with other providers - this is expected if they're not installed
            }
        }
    }

    Ok(all_instances)
}

// Helper function to get instances from a specific provider
fn get_instances_from_provider(provider_name: &str) -> VmResult<Vec<InstanceInfo>> {
    use vm_config::config::VmConfig;
    use vm_provider::get_provider;

    let config = VmConfig {
        provider: Some(provider_name.to_string()),
        ..Default::default()
    };

    match get_provider(config) {
        Ok(provider) => match provider.list_instances() {
            Ok(instances) => {
                debug!(
                    "Found {} instances from {} provider",
                    instances.len(),
                    provider_name
                );
                Ok(instances)
            }
            Err(e) => {
                debug!(
                    "Failed to list instances from {} provider: {}",
                    provider_name, e
                );
                Ok(Vec::new())
            }
        },
        Err(e) => {
            debug!("Provider {} not available: {}", provider_name, e);
            Ok(Vec::new())
        }
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn format_status(status: &str) -> String {
    // Normalize status strings across providers with icons
    let lower_status = status.to_lowercase();
    if lower_status.contains("running") || lower_status.contains("up") {
        "✅ Running".to_string()
    } else if lower_status.contains("stopped")
        || lower_status.contains("exited")
        || lower_status.contains("poweroff")
    {
        "🔴 Stopped".to_string()
    } else if lower_status.contains("paused") {
        "⏸️  Paused".to_string()
    } else {
        format!("❓ {}", status)
    }
}

fn format_uptime(uptime: &Option<String>) -> String {
    match uptime {
        Some(time) => time.clone(),
        None => "--".to_string(),
    }
}

/// Handle get sync directory
pub fn handle_get_sync_directory(provider: Box<dyn Provider>) {
    debug!("Getting sync directory for provider '{}'", provider.name());
    let sync_dir = provider.get_sync_directory();
    debug!("Sync directory: '{}'", sync_dir);
    println!("{}", sync_dir);
}

/// Handle VM destruction
pub async fn handle_destroy(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    _global_config: GlobalConfig,
    force: bool,
) -> VmResult<()> {
    // Get VM name from config for confirmation prompt
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("VM");

    let container_name = format!("{}-dev", vm_name);

    debug!(
        "Destroying VM: vm_name='{}', provider='{}', force={}",
        vm_name,
        provider.name(),
        force
    );

    // Determine the instance name for service cleanup
    let vm_instance_name = if let Some(container_name) = container {
        container_name.to_string()
    } else {
        container_name.clone()
    };

    // Check if container exists before showing confirmation
    let container_exists = std::process::Command::new("docker")
        .args(["inspect", &container_name])
        .output()
        .ok()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !container_exists {
        vm_println!("{}", MESSAGES.vm_destroy_cleanup_already_removed);

        // Clean up images even if container doesn't exist
        let _ = std::process::Command::new("docker")
            .args(["image", "rm", "-f", &format!("{}-image", vm_name)])
            .output();

        unregister_vm_services_helper(&vm_instance_name).await?;

        vm_println!("{}", MESSAGES.common_cleanup_complete);
        return Ok(());
    }

    let should_destroy = if force {
        debug!("Force flag set - skipping confirmation prompt");
        vm_println!("{}", msg!(MESSAGES.vm_destroy_force, name = vm_name));
        true
    } else {
        // Check status to show current state
        let is_running = provider.status(None).is_ok();

        vm_println!("{}", msg!(MESSAGES.vm_destroy_confirm, name = vm_name));
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_destroy_info_block,
                status = if is_running {
                    MESSAGES.common_status_running
                } else {
                    MESSAGES.common_status_stopped
                },
                container = container_name
            )
        );
        print!("{}", MESSAGES.vm_destroy_confirm_prompt);
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
        vm_println!("{}", MESSAGES.vm_destroy_progress);

        match provider.destroy(container) {
            Ok(()) => {
                vm_println!("{}", MESSAGES.common_configuring_services);
                unregister_vm_services_helper(&vm_instance_name).await?;

                vm_println!("{}", MESSAGES.vm_destroy_success);
                Ok(())
            }
            Err(e) => {
                vm_println!("\n❌ Destruction failed: {}", e);
                Err(VmError::from(e))
            }
        }
    } else {
        debug!("Destroy confirmation: response='no', cancelling destruction");
        vm_println!("{}", MESSAGES.vm_destroy_cancelled);
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
    handle_destroy(provider, container, config, global_config, *force).await
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
        vm_println!("{}", MESSAGES.vm_destroy_cross_no_instances);
        return Ok(());
    }

    // Show what will be destroyed
    vm_println!("{}", MESSAGES.vm_destroy_cross_list_header);
    for instance in &filtered_instances {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_destroy_cross_list_item,
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
                MESSAGES.vm_destroy_cross_confirm_prompt,
                count = filtered_instances.len().to_string()
            )
        );
        io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    };

    if !should_destroy {
        vm_println!("{}", MESSAGES.vm_destroy_cross_cancelled);
        return Ok(());
    }

    // Destroy each instance
    let mut success_count = 0;
    let mut error_count = 0;

    for instance in filtered_instances {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_destroy_cross_progress,
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
                        MESSAGES.vm_destroy_cross_success_item,
                        name = &instance.name
                    )
                );
                success_count += 1;
            }
            Err(e) => {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.vm_destroy_cross_failed,
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
            MESSAGES.vm_destroy_cross_complete,
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

/// Helper function to handle SSH start prompt interaction
fn handle_ssh_start_prompt(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    relative_path: &Path,
    vm_name: &str,
    _user: &str,
    _workspace_path: &str,
    _shell: &str,
) -> VmResult<Option<VmResult<()>>> {
    // Check if we're in an interactive terminal
    if !io::stdin().is_terminal() {
        vm_println!("{}", MESSAGES.vm_ssh_start_hint);
        return Ok(None);
    }

    print!("{}", MESSAGES.vm_ssh_start_prompt);
    io::stdout()
        .flush()
        .map_err(|e| VmError::general(e, "Failed to flush stdout"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| VmError::general(e, "Failed to read user input"))?;

    if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
        vm_println!("{}", MESSAGES.vm_ssh_start_aborted);
        return Ok(None);
    }

    // Start the VM
    vm_println!("{}", msg!(MESSAGES.vm_ssh_starting, name = vm_name));

    if let Err(e) = provider.start(container) {
        vm_println!(
            "{}",
            msg!(
                MESSAGES.vm_ssh_start_failed,
                name = vm_name,
                error = e.to_string()
            )
        );
        return Ok(None);
    }

    vm_println!("{}", msg!(MESSAGES.vm_ssh_reconnecting, name = vm_name));

    let retry_result = provider.ssh(container, relative_path);
    match &retry_result {
        Ok(()) => {
            vm_println!("{}", msg!(MESSAGES.vm_ssh_disconnected, name = vm_name));
        }
        Err(e) => {
            vm_println!("\n❌ SSH connection failed: {}", e);
        }
    }

    Ok(Some(retry_result.map_err(VmError::from)))
}

/// Handle SSH into VM
pub fn handle_ssh(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    path: Option<PathBuf>,
    config: VmConfig,
) -> VmResult<()> {
    let relative_path = path.unwrap_or_else(|| PathBuf::from("."));
    let workspace_path = config
        .project
        .as_ref()
        .and_then(|p| p.workspace_path.as_deref())
        .unwrap_or("/workspace");

    debug!(
        "SSH command: relative_path='{}', workspace_path='{}'",
        relative_path.display(),
        workspace_path
    );

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    // Default to "developer" for user since users field may not exist
    let user = "developer";

    let shell = config
        .terminal
        .as_ref()
        .and_then(|t| t.shell.as_deref())
        .unwrap_or("zsh");

    vm_println!("{}", msg!(MESSAGES.vm_ssh_connecting, name = vm_name));

    let result = provider.ssh(container, &relative_path);

    // Show message when SSH session ends
    match &result {
        Ok(()) => {
            vm_println!("{}", msg!(MESSAGES.vm_ssh_disconnected, name = vm_name));
        }
        Err(e) => {
            let error_str = e.to_string();

            // Check if VM doesn't exist first
            if error_str.contains("No such container") || error_str.contains("No such object") {
                vm_println!("{}", msg!(MESSAGES.vm_ssh_vm_not_found, name = vm_name));

                // Offer to create the VM
                if io::stdin().is_terminal() {
                    print!("{}", MESSAGES.vm_ssh_create_prompt);
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;

                    if matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                        // Actually create the VM
                        vm_println!("{}", msg!(MESSAGES.vm_ssh_creating, name = vm_name));

                        #[allow(clippy::excessive_nesting)]
                        match provider.create() {
                            Ok(()) => {
                                vm_println!(
                                    "{}",
                                    msg!(MESSAGES.vm_ssh_create_success, name = vm_name)
                                );

                                // Now try SSH again
                                return Ok(provider.ssh(container, &relative_path)?);
                            }
                            Err(create_err) => {
                                vm_println!(
                                    "{}",
                                    msg!(
                                        MESSAGES.vm_ssh_create_failed,
                                        name = vm_name,
                                        error = create_err.to_string()
                                    )
                                );
                                return Err(create_err.into());
                            }
                        }
                    } else {
                        vm_println!("\n💡 Create with: vm create");
                        vm_println!("💡 List existing VMs: vm list");
                    }
                } else {
                    vm_println!("\n💡 Create with: vm create");
                    vm_println!("💡 List existing VMs: vm list");
                }
                // Return a clean error that won't be misinterpreted by the main error handler
                return Err(VmError::vm_operation(
                    std::io::Error::new(std::io::ErrorKind::NotFound, "VM does not exist"),
                    Some(vm_name),
                    "ssh",
                ));
            }
            // Check if the error is because the container is not running
            else if error_str.contains("is not running")
                || error_str.contains("Container is not running")
                || (error_str.contains("docker")
                    && error_str.contains("exec")
                    && error_str.contains("exited with code 1")
                    && !error_str.contains("No such"))
            {
                vm_println!("{}", msg!(MESSAGES.vm_ssh_not_running, name = vm_name));

                // Handle interactive prompt
                if let Some(retry_result) = handle_ssh_start_prompt(
                    provider,
                    container,
                    &relative_path,
                    vm_name,
                    user,
                    workspace_path,
                    shell,
                )? {
                    return retry_result;
                }
            } else if error_str.contains("connection lost")
                || error_str.contains("connection failed")
            {
                vm_println!("{}", MESSAGES.vm_ssh_connection_lost);
            } else {
                // For other errors, show the actual error but clean up the message
                vm_println!("{}", MESSAGES.vm_ssh_session_ended);
            }
        }
    }

    Ok(result?)
}

/// Handle VM status check with enhanced dashboard
pub fn handle_status(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    debug!(
        "Status check: vm_name='{}', provider='{}'",
        vm_name,
        provider.name()
    );

    // Get comprehensive status report
    match provider.get_status_report(container) {
        Ok(report) => {
            display_status_dashboard(&report);
            Ok(())
        }
        Err(e) => {
            debug!("Status report failed: {}, falling back to basic status", e);
            // Fallback to basic stopped status display for providers that don't support enhanced status
            display_basic_stopped_status(vm_name, provider.name());
            Ok(()) // Don't propagate error, just show status
        }
    }
}

/// Display the compact status dashboard
fn display_status_dashboard(report: &VmStatusReport) {
    // Header with VM name
    println!("🖥️  {} ({})", report.name, report.provider);

    // Status line with uptime
    let status_icon = if report.is_running { "🟢" } else { "🔴" };
    let status_text = if report.is_running {
        "Running"
    } else {
        "Stopped"
    };

    if let Some(uptime) = &report.uptime {
        println!("   {} {} • Uptime: {}", status_icon, status_text, uptime);
    } else {
        println!("   {} {}", status_icon, status_text);
    }

    // Container ID (shortened)
    if let Some(id) = &report.container_id {
        let short_id = if id.len() > 12 { &id[..12] } else { id };
        println!("   📦 {}", short_id);
    }

    // Resource usage (if available)
    if report.is_running && has_resource_data(&report.resources) {
        display_resource_usage(&report.resources);
    }

    // Service health (if any services)
    if !report.services.is_empty() {
        display_service_health(&report.services);
    }

    // Connection hint
    if report.is_running {
        println!("\n💡 Connect: vm ssh");
    } else {
        println!("\n💡 Start: vm start");
    }
}

/// Display basic stopped status for providers without enhanced status support
fn display_basic_stopped_status(vm_name: &str, provider_name: &str) {
    println!("🖥️  {} ({})", vm_name, provider_name);
    println!("   🔴 Stopped");
    println!("   📦 Container not found");
    println!("\n💡 Start: vm start");
}

/// Check if resource data is available and meaningful
fn has_resource_data(resources: &vm_provider::ResourceUsage) -> bool {
    resources.cpu_percent.is_some()
        || resources.memory_used_mb.is_some()
        || resources.disk_used_gb.is_some()
}

/// Display resource usage information
fn display_resource_usage(resources: &vm_provider::ResourceUsage) {
    println!();

    // CPU usage
    if let Some(cpu) = resources.cpu_percent {
        let cpu_icon = if cpu > 80.0 {
            "🔥"
        } else if cpu > 50.0 {
            "⚡"
        } else {
            "💚"
        };
        println!("   {} CPU:    {:.1}%", cpu_icon, cpu);
    }

    // Memory usage
    if let Some(used) = resources.memory_used_mb {
        let memory_text = if let Some(limit) = resources.memory_limit_mb {
            let usage_pct = (used as f64 / limit as f64) * 100.0;
            let mem_icon = if usage_pct > 90.0 {
                "🔥"
            } else if usage_pct > 70.0 {
                "⚡"
            } else {
                "💚"
            };
            let used_display = format_memory_mb(used);
            let limit_display = format_memory_mb(limit);
            format!(
                "   {} Memory: {} / {} ({:.0}%)",
                mem_icon, used_display, limit_display, usage_pct
            )
        } else {
            let used_display = format_memory_mb(used);
            format!("   💚 Memory: {}", used_display)
        };
        println!("{}", memory_text);
    }

    // Disk usage
    if let Some(used) = resources.disk_used_gb {
        let disk_text = if let Some(total) = resources.disk_total_gb {
            let usage_pct = (used / total) * 100.0;
            let disk_icon = if usage_pct > 90.0 {
                "🔥"
            } else if usage_pct > 80.0 {
                "⚡"
            } else {
                "💚"
            };
            format!(
                "   {} Disk:   {:.1}GB / {:.1}GB ({:.0}%)",
                disk_icon, used, total, usage_pct
            )
        } else {
            format!("   💚 Disk:   {:.1}GB", used)
        };
        println!("{}", disk_text);
    }
}

/// Display service health information
fn display_service_health(services: &[vm_provider::ServiceStatus]) {
    println!();

    for service in services {
        let health_icon = if service.is_running { "🟢" } else { "🔴" };
        let port_info = match (service.port, service.host_port) {
            (Some(container_port), Some(host_port)) if container_port != host_port => {
                format!(" ({}→{})", host_port, container_port)
            }
            (Some(port), _) => format!(" ({})", port),
            _ => String::new(),
        };

        let service_line = if let Some(metrics) = &service.metrics {
            format!(
                "   {} {}{} • {}",
                health_icon, service.name, port_info, metrics
            )
        } else if let Some(error) = &service.error {
            format!(
                "   {} {}{} • {}",
                health_icon, service.name, port_info, error
            )
        } else {
            format!("   {} {}{}", health_icon, service.name, port_info)
        };

        println!("{}", service_line);
    }
}

/// Format memory size in MB to human-readable format
fn format_memory_mb(mb: u64) -> String {
    if mb >= 1024 {
        format!("{:.1}GB", mb as f64 / 1024.0)
    } else {
        format!("{}MB", mb)
    }
}

/// Handle command execution in VM
pub fn handle_exec(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    command: Vec<String>,
    config: VmConfig,
) -> VmResult<()> {
    debug!(
        "Executing command in VM: command={:?}, provider='{}'",
        command,
        provider.name()
    );

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    let cmd_display = command.join(" ");
    vm_println!(
        "{}",
        msg!(
            MESSAGES.vm_exec_header,
            name = vm_name,
            command = &cmd_display
        )
    );

    let result = provider.exec(container, &command);

    match &result {
        Ok(()) => {
            vm_println!("{}", MESSAGES.vm_exec_separator);
            vm_println!("{}", MESSAGES.vm_exec_success);
        }
        Err(e) => {
            vm_println!("{}", MESSAGES.vm_exec_separator);
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_exec_troubleshooting, error = e.to_string())
            );
        }
    }

    result.map_err(VmError::from)
}

/// Handle VM logs viewing
pub fn handle_logs(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
) -> VmResult<()> {
    debug!("Viewing VM logs: provider='{}'", provider.name());

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    let container_name = format!("{}-dev", vm_name);

    vm_println!("{}", msg!(MESSAGES.vm_logs_header, name = vm_name));

    let result = provider.logs(container);

    vm_println!(
        "{}",
        msg!(MESSAGES.vm_logs_footer, container = &container_name)
    );

    result.map_err(VmError::from)
}

/// Helper function to register VM services
async fn register_vm_services_helper(vm_name: &str, global_config: &GlobalConfig) -> VmResult<()> {
    if let Err(e) = get_service_manager()
        .register_vm_services(vm_name, global_config)
        .await
    {
        warn!("Failed to register VM services: {}", e);
        vm_println!(
            "{}",
            msg!(
                MESSAGES.common_services_config_failed,
                error = e.to_string()
            )
        );
        // Don't fail the operation if service registration fails
    } else {
        vm_println!("{}", MESSAGES.common_services_config_success);
    }
    Ok(())
}

/// Helper function to unregister VM services
async fn unregister_vm_services_helper(vm_name: &str) -> VmResult<()> {
    if let Err(e) = get_service_manager().unregister_vm_services(vm_name).await {
        warn!("Failed to unregister VM services: {}", e);
        vm_println!(
            "{}",
            msg!(
                MESSAGES.common_services_cleanup_failed,
                error = e.to_string()
            )
        );
        // Don't fail the operation if service cleanup fails
    } else {
        vm_println!("{}", MESSAGES.common_services_cleaned);
    }
    Ok(())
}
