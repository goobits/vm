// VM operation command handlers

use anyhow::Result;
use log::{debug, info, warn};
use std::path::PathBuf;

use vm_common::scoped_context;
use vm_config::config::VmConfig;
use vm_provider::{progress::{confirm_prompt, ProgressReporter, StatusFormatter}, Provider};

/// Handle VM creation
pub fn handle_create(provider: Box<dyn Provider>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "create" };
    info!("Starting VM creation");
    provider.create()
}

/// Handle VM start
pub fn handle_start(provider: Box<dyn Provider>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "start" };
    info!("Starting VM");
    provider.start()
}

/// Handle VM stop
pub fn handle_stop(provider: Box<dyn Provider>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "stop" };
    info!("Stopping VM");
    provider.stop()
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

/// Handle VM kill
pub fn handle_kill(provider: Box<dyn Provider>, container: Option<String>) -> Result<()> {
    let _op_guard = scoped_context! { "operation" => "kill" };
    warn!("Force killing VM processes: container={:?}", container);
    provider.kill(container.as_deref())
}

/// Handle get sync directory
pub fn handle_get_sync_directory(provider: Box<dyn Provider>) -> Result<()> {
    debug!("Getting sync directory for provider '{}'", provider.name());
    let sync_dir = provider.get_sync_directory()?;
    debug!("Sync directory: '{}'", sync_dir);
    println!("{}", sync_dir);
    Ok(())
}

/// Handle VM destruction
pub fn handle_destroy(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
    // Get VM name from config for confirmation prompt
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("VM");

    debug!(
        "Destroying VM: vm_name='{}', provider='{}'",
        vm_name,
        provider.name()
    );

    // Initialize progress reporter
    let progress = ProgressReporter::new();

    // Show confirmation prompt
    progress.phase_header("🗑️", "DESTROY PHASE");
    let confirmation_msg = format!(
        "├─ ⚠️  Are you sure you want to destroy {}? This will delete all data. (y/N): ",
        vm_name
    );

    if confirm_prompt(&confirmation_msg) {
        debug!("Destroy confirmation: response='yes', proceeding with destruction");
        progress.subtask("├─", "Proceeding with destruction...");
        let result = provider.destroy();
        match result {
            Ok(()) => progress.complete("└─", "VM destroyed successfully"),
            Err(e) => {
                progress.error("└─", &format!("Destruction failed: {}", e));
                return Err(e);
            }
        }
        result
    } else {
        debug!("Destroy confirmation: response='no', cancelling destruction");
        progress.error("└─", "Destruction cancelled");
        std::process::exit(1);
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
    let progress = ProgressReporter::new();
    let status_formatter = StatusFormatter::new();

    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    // Get memory and cpu info from config
    let memory = config.vm.as_ref().and_then(|vm| vm.memory);
    let cpus = config.vm.as_ref().and_then(|vm| vm.cpus);

    debug!(
        "Status check: vm_name='{}', provider='{}', memory={:?}, cpus={:?}",
        vm_name,
        provider.name(),
        memory,
        cpus
    );

    progress.phase_header("📊", "STATUS CHECK");
    progress.subtask("├─", "Checking VM status...");

    let result = provider.status();
    match result {
        Ok(()) => {
            debug!("Status check successful for VM '{}'", vm_name);
            progress.complete("└─", "Status check complete");

            // Format status information
            println!("\n");
            status_formatter.format_status(
                vm_name,
                "running", // This could be enhanced to get actual status
                provider.name(),
                memory,
                cpus,
            );
        }
        Err(e) => {
            debug!("Status check failed for VM '{}': {}", vm_name, e);
            progress.error("└─", &format!("Status check failed: {}", e));
            return Err(e);
        }
    }
    result
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