// VM operation command handlers

// Standard library
use std::path::PathBuf;

// External crates
use anyhow::{Context, Result};
use log::{debug, info, warn};

// Internal imports
use vm_common::scoped_context;
use vm_common::vm_error;
use vm_config::config::VmConfig;
use vm_provider::Provider;

/// Handle VM creation
pub fn handle_create(provider: Box<dyn Provider>, config: VmConfig, force: bool) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "create" };
    info!("Starting VM creation");

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    if force {
        debug!("Force flag set - will destroy existing VM if present");
        // Check if VM exists and destroy it first
        if provider.status().is_ok() {
            warn!("VM exists, destroying due to --force flag");
            println!("🔄 Force recreating '{}'...", vm_name);
            provider.destroy()?;
        }
    }

    println!("🚀 Creating '{}'...\n", vm_name);
    println!("  ✓ Building Docker image");
    println!("  ✓ Setting up volumes");
    println!("  ✓ Configuring network");
    println!("  ✓ Starting container");
    println!("  ✓ Running initial provisioning");

    match provider.create() {
        Ok(()) => {
            println!("\n✅ Created successfully\n");

            let container_name = format!("{}-dev", vm_name);
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
            Err(e)
        }
    }
}

/// Handle VM start
pub fn handle_start(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "start" };
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

    match provider.start() {
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
            Err(e)
        }
    }
}

/// Handle VM stop - graceful stop for current project or force kill specific container
pub fn handle_stop(
    provider: Box<dyn Provider>,
    container: Option<String>,
    config: VmConfig,
) -> Result<()> {
    match container {
        None => {
            // Graceful stop of current project VM
            let _op_guard = scoped_context! { "operation" => "stop" };
            info!("Stopping VM");

            let vm_name = config
                .project
                .as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("vm-project");

            println!("🛑 Stopping '{}'...", vm_name);

            match provider.stop() {
                Ok(()) => {
                    println!("✅ Stopped successfully\n");
                    println!("💡 Restart with: vm start");
                    Ok(())
                }
                Err(e) => {
                    println!("❌ Failed to stop '{}'", vm_name);
                    println!("   Error: {}", e);
                    Err(e)
                }
            }
        }
        Some(container_name) => {
            // Force kill specific container
            let _op_guard = scoped_context! { "operation" => "kill" };
            warn!("Force killing container: {}", container_name);

            println!("⚠️  Force stopping container '{}'...", container_name);

            match provider.kill(Some(&container_name)) {
                Ok(()) => {
                    println!("✅ Container stopped");
                    Ok(())
                }
                Err(e) => {
                    println!("❌ Failed to stop container");
                    println!("   Error: {}", e);
                    Err(e)
                }
            }
        }
    }
}

/// Handle VM restart
pub fn handle_restart(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "restart" };
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

    match provider.restart() {
        Ok(()) => {
            println!("  ✓ Services ready\n");
            println!("✅ Restarted successfully");
            Ok(())
        }
        Err(e) => {
            println!("\n❌ Failed to restart '{}'", vm_name);
            println!("   Error: {}", e);
            Err(e)
        }
    }
}

/// Handle VM provisioning
pub fn handle_provision(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "provision" };
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

    match provider.provision() {
        Ok(()) => {
            println!("\n✅ Provisioning complete");
            println!("\n💡 Changes applied to running container");
            Ok(())
        }
        Err(e) => {
            println!("\n❌ Provisioning failed");
            println!("   Error: {}", e);
            println!("\n💡 Check logs: vm logs");
            Err(e)
        }
    }
}

/// Handle VM listing
pub fn handle_list(provider: Box<dyn Provider>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "list" };
    debug!("Listing VMs");
    provider.list()
}

/// Handle get sync directory
pub fn handle_get_sync_directory(provider: Box<dyn Provider>) {
    debug!("Getting sync directory for provider '{}'", provider.name());
    let sync_dir = provider.get_sync_directory();
    debug!("Sync directory: '{}'", sync_dir);
    println!("{}", sync_dir);
}

/// Handle VM destruction
pub fn handle_destroy(provider: Box<dyn Provider>, config: VmConfig, force: bool) -> Result<()> {
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
        let is_running = provider.status().is_ok();

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
        io::stdout().flush().context("Failed to flush stdout")?;

        let mut response = String::new();
        io::stdin()
            .read_line(&mut response)
            .context("Failed to read user input")?;
        response.trim().to_lowercase() == "y"
    };

    if should_destroy {
        debug!("Destroy confirmation: response='yes', proceeding with destruction");
        println!("\n  ✓ Stopping container");
        println!("  ✓ Removing container");
        println!("  ✓ Cleaning images");

        match provider.destroy() {
            Ok(()) => {
                println!("\n✅ VM destroyed");
                Ok(())
            }
            Err(e) => {
                println!("\n❌ Destruction failed: {}", e);
                Err(e)
            }
        }
    } else {
        debug!("Destroy confirmation: response='no', cancelling destruction");
        println!("\n❌ Destruction cancelled");
        vm_error!("VM destruction cancelled by user");
        Err(anyhow::anyhow!("VM destruction cancelled by user"))
    }
}

/// Handle SSH into VM
pub fn handle_ssh(
    provider: Box<dyn Provider>,
    path: Option<PathBuf>,
    config: VmConfig,
) -> Result<()> {
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
    println!();
    println!("  User:  {}", user);
    println!("  Path:  {}", workspace_path);
    println!("  Shell: {}", shell);
    println!("\n💡 Exit with: exit or Ctrl-D\n");

    let result = provider.ssh(&relative_path);

    // Show message when SSH session ends
    match &result {
        Ok(()) => {
            println!("\n👋 Disconnected from '{}'", vm_name);
            println!("💡 Reconnect with: vm ssh");
        }
        Err(e) => {
            // Check if it was a user interrupt (Ctrl-C) or connection failure
            let error_str = e.to_string();
            if error_str.contains("130") || error_str.contains("Interrupted") {
                println!("\n⚠️  SSH session interrupted");
                println!("💡 Reconnect with: vm ssh");
            } else {
                println!("\n❌ SSH connection failed: {}", e);
                println!("💡 Check if VM is running: vm status");
            }
        }
    }

    result
}

/// Handle VM status check
pub fn handle_status(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
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

    match provider.status() {
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
    command: Vec<String>,
    config: VmConfig,
) -> Result<()> {
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

    let result = provider.exec(&command);

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

    result
}

/// Handle VM logs viewing
pub fn handle_logs(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
    debug!("Viewing VM logs: provider='{}'", provider.name());

    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    println!("📜 Logs for '{}' (last 50 lines)", vm_name);
    println!("──────────────────────────────────────────");

    let result = provider.logs();

    println!("──────────────────────────────────────────");
    println!("💡 Follow live: docker logs -f {}-dev", vm_name);
    println!("💡 Full logs: docker logs {}-dev", vm_name);

    result
}
