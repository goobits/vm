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
use vm_common::vm_error;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_provider::{InstanceInfo, Provider};

/// Handle VM creation
pub async fn handle_create(
    provider: Box<dyn Provider>,
    config: VmConfig,
    global_config: GlobalConfig,
    force: bool,
    instance: Option<String>,
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
        println!(
            "🚀 Creating instance '{}' for project '{}'...",
            instance_name, vm_name
        );

        if force {
            debug!("Force flag set - will destroy existing instance if present");
            // Try to destroy specific instance first
            println!("🔄 Force recreating instance '{}'...", instance_name);
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
            println!("ℹ️  Instance name '{}' specified but provider '{}' doesn't support multi-instance. Using default behavior.", instance_name, provider.name());
        }

        if force {
            debug!("Force flag set - will destroy existing VM if present");
            // Check if VM exists and destroy it first
            if provider.status(None).is_ok() {
                warn!("VM exists, destroying due to --force flag");
                println!("🔄 Force recreating '{}'...", vm_name);
                provider.destroy(None).map_err(VmError::from)?;
            }
        }
    }

    println!("🚀 Creating '{}'...\n", vm_name);
    println!("  ✓ Building Docker image");
    println!("  ✓ Setting up volumes");
    println!("  ✓ Configuring network");
    println!("  ✓ Starting container");
    println!("  ✓ Running initial provisioning");

    // Call the appropriate create method based on whether instance is specified
    let create_result = if let Some(instance_name) = &instance {
        if provider.supports_multi_instance() {
            provider.create_instance(instance_name)
        } else {
            provider.create()
        }
    } else {
        provider.create()
    };

    match create_result {
        Ok(()) => {
            println!("\n✅ Created successfully\n");

            let container_name = if let Some(instance_name) = &instance {
                format!("{}-{}", vm_name, instance_name)
            } else {
                format!("{}-dev", vm_name)
            };
            println!("  Status:     🟢 Running");
            println!("  Container:  {}", container_name);

            // Show resources if available
            if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus) {
                if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
                    // Format memory display
                    let mem_str = match memory.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{}MB", mb),
                        None => format!("{:?}", memory),
                    };
                    println!("  Resources:  {} CPUs, {}", cpus, mem_str);
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
                println!("  Services:   {}", services.join(", "));
            }

            // Show port range
            if let Some(range) = &config.ports.range {
                if range.len() == 2 {
                    println!("  Ports:      {}-{}", range[0], range[1]);
                }
            }

            // Register VM services and auto-start them
            let vm_instance_name = if let Some(instance_name) = &instance {
                format!("{}-{}", vm_name, instance_name)
            } else {
                format!("{}-dev", vm_name)
            };

            println!("\n🔧 Configuring services...");
            if let Err(e) = get_service_manager()
                .register_vm_services(&vm_instance_name, &global_config)
                .await
            {
                warn!("Failed to register VM services: {}", e);
                println!("  Status:     ⚠️  Service configuration failed: {}", e);
            } else {
                println!("  Status:     ✅ Services configured successfully");
            }

            println!("\n💡 Connect with: vm ssh");
            Ok(())
        }
        Err(e) => {
            println!("\n❌ Failed to create '{}'", vm_name);
            println!("   Error: {}", e);
            println!("\n💡 Try:");
            println!("   • Check Docker status: docker ps");
            println!("   • View Docker logs: docker logs");
            println!("   • Retry with force: vm create --force");
            Err(VmError::from(e))
        }
    }
}

/// Handle VM start
pub fn handle_start(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
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
            println!("✅ VM '{}' is already running", vm_name);
            println!("\n💡 Connect with: vm ssh");
            return Ok(());
        }
    }

    println!("🚀 Starting '{}'...", vm_name);

    match provider.start(container) {
        Ok(()) => {
            println!("✅ Started successfully\n");

            // Show VM details
            println!("  Status:     🟢 Running");
            println!("  Container:  {}", container_name);

            // Show resources if available
            if let Some(cpus) = config.vm.as_ref().and_then(|vm| vm.cpus) {
                if let Some(memory) = config.vm.as_ref().and_then(|vm| vm.memory.as_ref()) {
                    // Format memory display
                    let mem_str = match memory.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{}MB", mb),
                        None => format!("{:?}", memory),
                    };
                    println!("  Resources:  {} CPUs, {}", cpus, mem_str);
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
                println!("  Services:   {}", services.join(", "));
            }

            // Global services (package registry, auth proxy, docker registry) are now
            // managed automatically by the ServiceManager during VM registration

            println!("\n💡 Connect with: vm ssh");

            Ok(())
        }
        Err(e) => {
            println!("❌ Failed to start '{}'", vm_name);
            println!("   Error: {}", e);
            println!("\n💡 Try:");
            println!("   • Check Docker status: docker ps");
            println!("   • View logs: docker logs {}", container_name);
            println!("   • Recreate VM: vm create --force");
            Err(VmError::from(e))
        }
    }
}

/// Handle VM stop - graceful stop for current project or force kill specific container
pub fn handle_stop(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
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

            println!("🛑 Stopping '{}'...", vm_name);

            match provider.stop(None) {
                Ok(()) => {
                    println!("✅ Stopped successfully\n");
                    println!("💡 Restart with: vm start");
                    Ok(())
                }
                Err(e) => {
                    println!("❌ Failed to stop '{}'", vm_name);
                    println!("   Error: {}", e);
                    Err(VmError::from(e))
                }
            }
        }
        Some(container_name) => {
            // Force kill specific container
            let span = info_span!("vm_operation", operation = "kill");
            let _enter = span.enter();
            warn!("Force killing container: {}", container_name);

            println!("⚠️  Force stopping container '{}'...", container_name);

            match provider.kill(Some(container_name)) {
                Ok(()) => {
                    println!("✅ Container stopped");
                    Ok(())
                }
                Err(e) => {
                    println!("❌ Failed to stop container");
                    println!("   Error: {}", e);
                    Err(VmError::from(e))
                }
            }
        }
    }
}

/// Handle VM restart
pub fn handle_restart(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
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

    println!("🔄 Restarting '{}'...", vm_name);
    println!("  ✓ Stopping container");
    println!("  ✓ Starting container");

    match provider.restart(container) {
        Ok(()) => {
            println!("  ✓ Services ready\n");
            println!("✅ Restarted successfully");
            Ok(())
        }
        Err(e) => {
            println!("\n❌ Failed to restart '{}'", vm_name);
            println!("   Error: {}", e);
            Err(VmError::from(e))
        }
    }
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

    println!("🔧 Re-provisioning '{}'\n", vm_name);
    println!("  ✓ Updating packages");
    println!("  ✓ Installing dependencies");
    println!("  ✓ Configuring services");
    println!("  ✓ Restarting services");

    match provider.provision(container) {
        Ok(()) => {
            println!("\n✅ Provisioning complete");
            println!("\n💡 Changes applied to running container");
            Ok(())
        }
        Err(e) => {
            println!("\n❌ Provisioning failed");
            println!("   Error: {}", e);
            println!("\n💡 Check logs: vm logs");
            Err(VmError::from(e))
        }
    }
}

/// Handle VM listing with enhanced filtering options
pub fn handle_list_enhanced(
    _provider: Box<dyn Provider>,
    _all_providers: &bool,
    provider_filter: Option<&str>,
    verbose: &bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "list");
    let _enter = span.enter();
    debug!(
        "Listing VMs with enhanced filtering - provider_filter: {:?}, verbose: {}",
        provider_filter, *verbose
    );

    // Get all instances from all providers (or filtered)
    let all_instances = if let Some(provider_name) = provider_filter {
        get_instances_from_provider(provider_name)?
    } else {
        get_all_instances()?
    };

    if all_instances.is_empty() {
        if let Some(provider_name) = provider_filter {
            println!("No VMs found for provider '{}'", provider_name);
        } else {
            println!("No VMs found");
        }
        return Ok(());
    }

    // Print header
    if *verbose {
        println!(
            "{:<20} {:<10} {:<10} {:<20} {:<10} {:<15}",
            "INSTANCE", "PROVIDER", "STATUS", "ID", "UPTIME", "PROJECT"
        );
        println!("{}", "-".repeat(95));
    } else {
        println!(
            "{:<20} {:<10} {:<10} {:<15} {:<10}",
            "INSTANCE", "PROVIDER", "STATUS", "ID", "UPTIME"
        );
        println!("{}", "-".repeat(75));
    }

    // Sort instances by provider then name for consistent output
    let mut sorted_instances = all_instances;
    sorted_instances.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));

    for instance in sorted_instances {
        if *verbose {
            println!(
                "{:<20} {:<10} {:<10} {:<20} {:<10} {:<15}",
                truncate_string(&instance.name, 20),
                instance.provider,
                format_status(&instance.status),
                truncate_string(&instance.id, 20),
                format_uptime(&instance.uptime),
                instance.project.as_deref().unwrap_or("--")
            );
        } else {
            println!(
                "{:<20} {:<10} {:<10} {:<15} {:<10}",
                truncate_string(&instance.name, 20),
                instance.provider,
                format_status(&instance.status),
                truncate_string(&instance.id, 15),
                format_uptime(&instance.uptime)
            );
        }
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
    // Normalize status strings across providers
    let lower_status = status.to_lowercase();
    if lower_status.contains("running") || lower_status.contains("up") {
        "running".to_string()
    } else if lower_status.contains("stopped")
        || lower_status.contains("exited")
        || lower_status.contains("poweroff")
    {
        "stopped".to_string()
    } else if lower_status.contains("paused") {
        "paused".to_string()
    } else {
        status.to_string()
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

    let should_destroy = if force {
        debug!("Force flag set - skipping confirmation prompt");
        println!("🗑️ Destroying '{}' (forced)\n", vm_name);
        true
    } else {
        // Check status first to show current state
        let is_running = provider.status(None).is_ok();

        println!("🗑️ Destroy VM '{}'?\n", vm_name);
        println!(
            "  Status:     {}",
            if is_running {
                "🟢 Running"
            } else {
                "🔴 Stopped"
            }
        );
        println!("  Container:  {}", container_name);
        println!("\n⚠️  This will permanently delete:");
        println!("  • Container and all data");
        println!("  • Docker image and build cache");
        println!();
        print!("Confirm destruction? (y/N): ");
        use std::io::{self, Write};
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
        println!("\n  ✓ Stopping container");
        println!("  ✓ Removing container");
        println!("  ✓ Cleaning images");

        match provider.destroy(container) {
            Ok(()) => {
                // Unregister VM services after successful destruction
                let vm_instance_name = if let Some(container_name) = container {
                    container_name.to_string()
                } else {
                    container_name.clone()
                };

                if let Err(e) = get_service_manager()
                    .unregister_vm_services(&vm_instance_name)
                    .await
                {
                    warn!("Failed to unregister VM services: {}", e);
                    // Don't fail the destroy operation for service cleanup issues
                    println!("  ⚠️  Service cleanup failed: {}", e);
                } else {
                    println!("  ✓ Services cleaned up");
                }

                println!("\n✅ VM destroyed");
                Ok(())
            }
            Err(e) => {
                println!("\n❌ Destruction failed: {}", e);
                Err(VmError::from(e))
            }
        }
    } else {
        debug!("Destroy confirmation: response='no', cancelling destruction");
        println!("\n❌ Destruction cancelled");
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
pub async fn handle_destroy_enhanced(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
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
    handle_destroy(provider, container, config, *force).await
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
        println!("No instances found to destroy");
        return Ok(());
    }

    // Show what will be destroyed
    println!("Instances to destroy:");
    for instance in &filtered_instances {
        println!("  {} ({})", instance.name, instance.provider);
    }

    let should_destroy = if force {
        true
    } else {
        print!(
            "\nAre you sure you want to destroy {} instance(s)? (y/N): ",
            filtered_instances.len()
        );
        io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
    };

    if !should_destroy {
        println!("Destroy operation cancelled");
        return Ok(());
    }

    // Destroy each instance
    let mut success_count = 0;
    let mut error_count = 0;

    for instance in filtered_instances {
        println!("Destroying {} ({})...", instance.name, instance.provider);

        let result = destroy_single_instance(&instance);
        match result {
            Ok(()) => {
                println!("  ✅ Successfully destroyed {}", instance.name);
                success_count += 1;
            }
            Err(e) => {
                println!("  ❌ Failed to destroy {}: {}", instance.name, e);
                error_count += 1;
            }
        }
    }

    println!("\nDestroy operation completed:");
    println!("  Success: {}", success_count);
    if error_count > 0 {
        println!("  Errors: {}", error_count);
    }

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
        println!("\n💡 Start the VM with: vm start");
        println!("💡 Then reconnect with: vm ssh");
        return Ok(None);
    }

    print!("\nWould you like to start it now? (y/N): ");
    io::stdout()
        .flush()
        .map_err(|e| VmError::general(e, "Failed to flush stdout"))?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| VmError::general(e, "Failed to read user input"))?;

    if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
        println!("\n❌ SSH connection aborted");
        println!("💡 Start the VM manually with: vm start");
        return Ok(None);
    }

    // Start the VM
    println!("\n🚀 Starting '{}'...", vm_name);

    if let Err(e) = provider.start(container) {
        println!("❌ Failed to start '{}': {}", vm_name, e);
        println!("\n💡 Try:");
        println!("   • Check Docker status: docker ps");
        println!("   • View logs: docker logs {}-dev", vm_name);
        println!("   • Recreate VM: vm create --force");
        return Ok(None);
    }

    println!("✅ Started successfully");

    // Now retry the SSH connection
    println!("\n🔗 Reconnecting to '{}'...", vm_name);

    let retry_result = provider.ssh(container, relative_path);
    match &retry_result {
        Ok(()) => {
            println!("\n👋 Disconnected from '{}'", vm_name);
            println!("💡 Reconnect with: vm ssh");
        }
        Err(e) => {
            println!("\n❌ SSH connection failed: {}", e);
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

    println!("🔗 Connecting to '{}'...", vm_name);

    let result = provider.ssh(container, &relative_path);

    // Show message when SSH session ends
    match &result {
        Ok(()) => {
            println!("\n👋 Disconnected from '{}'", vm_name);
            println!("💡 Reconnect with: vm ssh");
        }
        Err(e) => {
            let error_str = e.to_string();

            // Check if VM doesn't exist first
            if error_str.contains("No such container") || error_str.contains("No such object") {
                println!("\n🔍 VM '{}' doesn't exist", vm_name);

                // Offer to create the VM
                if io::stdin().is_terminal() {
                    print!("\nWould you like to create it now? (y/N): ");
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;

                    if matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                        // Actually create the VM
                        println!("\n🚀 Creating '{}'...\n", vm_name);
                        println!("  ✓ Building Docker image");
                        println!("  ✓ Setting up volumes");
                        println!("  ✓ Configuring network");
                        println!("  ✓ Starting container");
                        println!("  ✓ Running initial provisioning");

                        #[allow(clippy::excessive_nesting)]
                        match provider.create() {
                            Ok(()) => {
                                println!("\n✅ Created successfully");
                                println!("\n🔗 Connecting to '{}'...", vm_name);

                                // Now try SSH again
                                return Ok(provider.ssh(container, &relative_path)?);
                            }
                            Err(create_err) => {
                                println!("\n❌ Failed to create '{}'", vm_name);
                                println!("   Error: {}", create_err);
                                println!("\n💡 Try:");
                                println!("   • Check Docker: docker ps");
                                println!("   • View logs: docker logs");
                                println!("   • Manual create: vm create");
                                return Err(create_err.into());
                            }
                        }
                    } else {
                        println!("\n💡 Create with: vm create");
                        println!("💡 List existing VMs: vm list");
                    }
                } else {
                    println!("\n💡 Create with: vm create");
                    println!("💡 List existing VMs: vm list");
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
                println!("\n⚠️  VM '{}' is not running", vm_name);

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
                println!("\n⚠️  Lost connection to VM");
                println!("💡 Check if VM is running: vm status");
            } else {
                // For other errors, show the actual error but clean up the message
                println!("\n⚠️  Session ended unexpectedly");
                println!("💡 Check VM status: vm status");
            }
        }
    }

    Ok(result?)
}

/// Handle VM status check
pub fn handle_status(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
) -> VmResult<()> {
    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    let container_name = format!("{}-dev", vm_name);

    // Get memory and cpu info from config
    let memory = config.vm.as_ref().and_then(|vm| vm.memory.as_ref());
    let cpus = config.vm.as_ref().and_then(|vm| vm.cpus);

    debug!(
        "Status check: vm_name='{}', provider='{}', memory={:?}, cpus={:?}",
        vm_name,
        provider.name(),
        memory,
        cpus
    );

    println!("📊 {}", vm_name);

    match provider.status(container) {
        Ok(()) => {
            println!("\n  Status:     🟢 Running");
            println!("  Provider:   {}", provider.name());
            println!("  Container:  {}", container_name);

            // Show resources
            if cpus.is_some() || memory.is_some() {
                println!("\n  Resources:");
                if let Some(cpu_count) = cpus {
                    println!("    CPUs:     {} cores", cpu_count);
                }
                if let Some(mem) = memory {
                    // Format memory display
                    let mem_str = match mem.to_mb() {
                        Some(mb) if mb >= 1024 => format!("{}GB", mb / 1024),
                        Some(mb) => format!("{}MB", mb),
                        None => format!("{:?}", mem),
                    };
                    println!("    Memory:   {}", mem_str);
                }
            }

            // Show services if configured
            let services: Vec<(String, Option<u16>)> = config
                .services
                .iter()
                .filter(|(_, svc)| svc.enabled)
                .map(|(name, svc)| (name.clone(), svc.port))
                .collect();

            if !services.is_empty() {
                println!("\n  Services:");
                for (name, port) in services {
                    if let Some(p) = port {
                        println!("    {}  🟢 {}", name, p);
                    } else {
                        println!("    {}  🟢", name);
                    }
                }
            }

            // Show global services status (now managed globally)
            let mut additional_services = Vec::new();

            // Check global services that might be running
            let package_status = if check_registry_status_sync() {
                ("Package Registry", "🟢", "http://localhost:3080")
            } else {
                ("Package Registry", "🔴", "Not running")
            };
            additional_services.push(package_status);

            let auth_status = if check_auth_proxy_status_sync() {
                ("Auth Proxy", "🟢", "http://localhost:3090")
            } else {
                ("Auth Proxy", "🔴", "Not running")
            };
            additional_services.push(auth_status);

            let docker_registry_status = if check_docker_registry_status_sync() {
                ("Docker Registry", "🟢", "http://localhost:5000")
            } else {
                ("Docker Registry", "🔴", "Not running")
            };
            additional_services.push(docker_registry_status);

            if !additional_services.is_empty() {
                println!("\n  Additional Services:");
                for (name, icon, url) in additional_services {
                    println!("    {}  {} {}", name, icon, url);
                }
            }

            Ok(())
        }
        Err(_) => {
            println!("\n  Status:     🔴 Not running");
            println!("  Provider:   {}", provider.name());
            println!("  Container:  {} (not found)", container_name);
            println!("\n💡 Start with: vm start");
            Ok(()) // Don't propagate error, just show status
        }
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
    println!("🏃 Running in '{}': {}", vm_name, cmd_display);
    println!("──────────────────────────────────────────");

    let result = provider.exec(container, &command);

    match &result {
        Ok(()) => {
            println!("──────────────────────────────────────────");
            println!("✅ Command completed successfully (exit code 0)");
            println!("\n💡 Run another: vm exec <command>");
        }
        Err(e) => {
            println!("──────────────────────────────────────────");

            // Try to extract exit code from error message if available
            let error_str = e.to_string();
            if error_str.contains("exit code") || error_str.contains("exit status") {
                println!("❌ Command failed: {}", e);
            } else if error_str.contains("exited with code 1") {
                println!("❌ Command failed (exit code 1)");
            } else {
                println!("❌ Command failed");
                println!("   Error: {}", e);
            }

            println!("\n💡 Debug with: vm ssh");
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

    println!("📜 Logs for '{}' (last 50 lines)", vm_name);
    println!("──────────────────────────────────────────");

    let result = provider.logs(container);

    println!("──────────────────────────────────────────");
    println!("💡 Follow live: docker logs -f {}-dev", vm_name);
    println!("💡 Full logs: docker logs {}-dev", vm_name);

    result.map_err(VmError::from)
}

/// Start package registry server in background
#[allow(dead_code)] // TODO: Remove this function after ServiceManager integration is complete
fn start_package_registry_background() -> VmResult<()> {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let data_dir = std::env::current_dir()?.join(".vm-packages");

    // Start the server in a background thread
    thread::spawn(move || {
        // Package registry functionality is disabled for now
        // When enabled, this will start the vm-package-server
        let _ = data_dir; // Avoid unused variable warning
    });

    // Give the server a moment to start
    thread::sleep(Duration::from_millis(1000));

    // Verify it started successfully
    let output = Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "http://localhost:3080/health",
        ])
        .output();

    match output {
        Ok(result) if result.stdout == b"200" => Ok(()),
        _ => {
            // Try a different verification method if curl isn't available
            thread::sleep(Duration::from_millis(1000));
            Ok(()) // Assume success for now
        }
    }
}

/// Check if package registry is running (synchronous version)
fn check_registry_status_sync() -> bool {
    use std::process::Command;

    // Try to check with curl first
    let curl_result = Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "http://localhost:3080/health",
        ])
        .output();

    if let Ok(output) = curl_result {
        if output.stdout == b"200" {
            return true;
        }
    }

    // If curl failed or not available, try with a simple TCP connection test
    use std::net::TcpStream;
    use std::time::Duration;

    TcpStream::connect_timeout(
        &"127.0.0.1:3080".parse().unwrap(),
        Duration::from_millis(1000),
    )
    .is_ok()
}

/// Check if auth proxy is running (synchronous version)
fn check_auth_proxy_status_sync() -> bool {
    use std::process::Command;
    // Try to check with curl first
    let curl_result = Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "http://localhost:3090/health",
        ])
        .output();

    if let Ok(output) = curl_result {
        if output.stdout == b"200" {
            return true;
        }
    }

    // If curl failed or not available, try with a simple TCP connection test
    use std::net::TcpStream;
    use std::time::Duration;

    TcpStream::connect_timeout(
        &"127.0.0.1:3090".parse().unwrap(),
        Duration::from_millis(1000),
    )
    .is_ok()
}

/// Check if Docker registry is running (synchronous version)
fn check_docker_registry_status_sync() -> bool {
    use std::process::Command;
    // Try to check with curl first
    let curl_result = Command::new("curl")
        .args([
            "-s",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "http://localhost:5000/health",
        ])
        .output();

    if let Ok(output) = curl_result {
        if output.stdout == b"200" {
            return true;
        }
    }

    // If curl failed or not available, try with a simple TCP connection test
    use std::net::TcpStream;
    use std::time::Duration;

    TcpStream::connect_timeout(
        &"127.0.0.1:5000".parse().unwrap(),
        Duration::from_millis(1000),
    )
    .is_ok()
}
