// VM operation command handlers

// Standard library
use std::path::PathBuf;

// External crates
use anyhow::Result;
use log::{debug, info, warn};

// Internal imports
use vm_common::scoped_context;
use vm_common::vm_error;
use vm_config::config::VmConfig;
use vm_provider::{
    progress::{confirm_prompt, ProgressReporter, StatusFormatter},
    Provider,
};

/// Handle VM creation
pub fn handle_create(provider: Box<dyn Provider>, force: bool) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "create" };
    info!("Starting VM creation");

    if force {
        debug!("Force flag set - will destroy existing VM if present");
        // Check if VM exists and destroy it first
        if provider.status().is_ok() {
            warn!("VM exists, destroying due to --force flag");
            provider.destroy()?;
        }
    }

    provider.create()
}

/// Handle VM start
pub fn handle_start(provider: Box<dyn Provider>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "start" };
    info!("Starting VM");
    provider.start()
}

/// Handle VM stop - graceful stop for current project or force kill specific container
pub fn handle_stop(provider: Box<dyn Provider>, container: Option<String>) -> Result<()> {
    match container {
        None => {
            // Graceful stop of current project VM
            let _op_guard = scoped_context! { "operation" => "stop" };
            info!("Stopping VM");
            provider.stop()
        }
        Some(container_name) => {
            // Force kill specific container
            let _op_guard = scoped_context! { "operation" => "kill" };
            warn!("Force killing container: {}", container_name);
            provider.kill(Some(&container_name))
        }
    }
}

/// Handle VM restart
pub fn handle_restart(provider: Box<dyn Provider>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "restart" };
    info!("Restarting VM");
    provider.restart()
}

/// Handle VM provisioning
pub fn handle_provision(provider: Box<dyn Provider>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "provision" };
    info!("Re-running VM provisioning");
    provider.provision()
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

    debug!(
        "Destroying VM: vm_name='{}', provider='{}', force={}",
        vm_name,
        provider.name(),
        force
    );

    // Initialize progress reporter (kept for potential future use)
    let _progress = ProgressReporter::new();

    let should_destroy = if force {
        debug!("Force flag set - skipping confirmation prompt");
        ProgressReporter::phase_header("🗑️", "DESTROY PHASE (FORCED)");
        true
    } else {
        // Show confirmation prompt
        ProgressReporter::phase_header("🗑️", "DESTROY PHASE");
        let confirmation_msg = format!(
            "├─ ⚠️  Are you sure you want to destroy {}? This will delete all data. (y/N): ",
            vm_name
        );
        confirm_prompt(&confirmation_msg)
    };

    if should_destroy {
        debug!("Destroy confirmation: response='yes', proceeding with destruction");
        ProgressReporter::subtask("├─", "Proceeding with destruction...");

        match provider.destroy() {
            Ok(()) => {
                ProgressReporter::complete("└─", "VM destroyed successfully");
                Ok(())
            }
            Err(e) => {
                ProgressReporter::error("└─", &format!("Destruction failed: {}", e));
                Err(e)
            }
        }
    } else {
        debug!("Destroy confirmation: response='no', cancelling destruction");
        ProgressReporter::error("└─", "Destruction cancelled");
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

    provider.ssh(&relative_path)
}

/// Handle VM status check
pub fn handle_status(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
    // Enhanced status reporting using StatusFormatter
    let _progress = ProgressReporter::new();

    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    // Get memory and cpu info from config
    let memory = config
        .vm
        .as_ref()
        .and_then(|vm| vm.memory.as_ref().and_then(|m| m.to_mb()));
    let cpus = config.vm.as_ref().and_then(|vm| vm.cpus);

    debug!(
        "Status check: vm_name='{}', provider='{}', memory={:?}, cpus={:?}",
        vm_name,
        provider.name(),
        memory,
        cpus
    );

    ProgressReporter::phase_header("📊", "STATUS CHECK");
    ProgressReporter::subtask("├─", "Checking VM status...");

    match provider.status() {
        Ok(()) => {
            debug!("Status check successful for VM '{}'", vm_name);
            ProgressReporter::complete("└─", "Status check complete");

            // Format status information
            println!("\n");
            StatusFormatter::format_status(
                vm_name,
                "running", // This could be enhanced to get actual status
                provider.name(),
                memory,
                cpus,
            );
            Ok(())
        }
        Err(e) => {
            debug!("Status check failed for VM '{}': {}", vm_name, e);
            ProgressReporter::error("└─", &format!("Status check failed: {}", e));
            Err(e)
        }
    }
}

/// Handle command execution in VM
pub fn handle_exec(provider: Box<dyn Provider>, command: Vec<String>) -> Result<()> {
    debug!(
        "Executing command in VM: command={:?}, provider='{}'",
        command,
        provider.name()
    );
    provider.exec(&command)
}

/// Handle VM logs viewing
pub fn handle_logs(provider: Box<dyn Provider>) -> Result<()> {
    debug!("Viewing VM logs: provider='{}'", provider.name());
    provider.logs()
}
