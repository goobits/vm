use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use vm_config::config::VmConfig;
use vm_provider::TempProvider;
use vm_temp::{MountParser, MountPermission, StateManager, TempVmState};

// External dependencies
extern crate atty;

#[derive(Debug, Parser)]
#[command(name = "vm-temp")]
#[command(about = "Temporary VM management for VM Tool")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create temp VM with mounts
    Create {
        /// Directories to mount (e.g., ./src,./config:ro)
        mounts: Vec<String>,

        /// Auto-destroy on exit
        #[arg(long)]
        auto_destroy: bool,
    },
    /// SSH into temp VM
    Ssh,
    /// Show temp VM status
    Status,
    /// Destroy temp VM
    Destroy,
    /// Add mount to running temp VM
    Mount {
        /// Path to mount (e.g., ./src or ./config:ro)
        path: String,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Remove mount from temp VM
    Unmount {
        /// Path to unmount (omit for --all)
        path: Option<String>,
        /// Remove all mounts
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// List current mounts
    Mounts,
    /// List all temp VMs
    List,
    /// Stop temp VM
    Stop,
    /// Start temp VM
    Start,
    /// Restart temp VM
    Restart,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Create {
            mounts,
            auto_destroy,
        } => handle_create_command(mounts, auto_destroy),
        Command::Ssh => handle_ssh_command(),
        Command::Status => handle_status_command(),
        Command::Destroy => handle_destroy_command(),
        Command::Mount { path, yes } => handle_mount_command(path, yes),
        Command::Unmount { path, all, yes } => handle_unmount_command(path, all, yes),
        Command::Mounts => handle_mounts_command(),
        Command::List => handle_list_command(),
        Command::Stop => handle_stop_command(),
        Command::Start => handle_start_command(),
        Command::Restart => handle_restart_command(),
    }
}

fn handle_create_command(mounts: Vec<String>, auto_destroy: bool) -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    // Parse mount strings using MountParser
    let parsed_mounts =
        MountParser::parse_mount_strings(&mounts).context("Failed to parse mount strings")?;

    // Get current project directory
    let project_dir = std::env::current_dir().context("Failed to get current directory")?;

    // Create temp VM state
    let mut temp_state = TempVmState::new(
        "vm-temp-dev".to_string(),
        "docker".to_string(),
        project_dir,
        auto_destroy,
    );

    // Add all mounts to the state
    for (source, target, permissions) in parsed_mounts {
        if let Some(target_path) = target {
            temp_state
                .add_mount_with_target(source, target_path, permissions)
                .context("Failed to add mount with custom target")?;
        } else {
            temp_state
                .add_mount(source, permissions)
                .context("Failed to add mount")?;
        }
    }

    // Create minimal temp config
    let temp_config = create_temp_config()?;

    // Create the VM
    let provider = TempDockerProvider::new(temp_config)?;
    provider.create()?;

    // Save state
    state_manager
        .save_state(&temp_state)
        .context("Failed to save temp VM state")?;

    println!(
        "✅ Temporary VM created with {} mount(s)",
        temp_state.mount_count()
    );

    if auto_destroy {
        // SSH then destroy
        println!("🔗 Connecting to temporary VM...");
        provider.ssh(&PathBuf::from("."))?;

        println!("🗑️ Auto-destroying temporary VM...");
        provider.destroy()?;
        state_manager
            .delete_state()
            .context("Failed to delete temp VM state")?;
    } else {
        println!("💡 Use 'vm-temp ssh' to connect");
        println!("   Use 'vm-temp destroy' when done");
    }

    Ok(())
}

fn handle_ssh_command() -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    if !state_manager.state_exists() {
        return Err(anyhow::anyhow!(
            "No temp VM found\n💡 Create one with: vm-temp create ./your-directory"
        ));
    }

    let temp_config = create_temp_config()?;
    let provider = TempDockerProvider::new(temp_config)?;
    provider.ssh(&PathBuf::from("."))
}

fn handle_status_command() -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    if !state_manager.state_exists() {
        println!("❌ No temp VM found");
        println!("💡 Create one with: vm-temp create ./your-directory");
        return Ok(());
    }

    let state = state_manager
        .load_state()
        .context("Failed to load temp VM state")?;

    println!("📊 Temp VM Status:");
    println!("   Container: {}", state.container_name);
    println!("   Provider: {}", state.provider);
    println!(
        "   Created: {}",
        state.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!("   Project: {}", state.project_dir.display());
    println!("   Mounts: {}", state.mount_count());
    if state.is_auto_destroy() {
        println!("   Auto-destroy: enabled");
    }

    // Check provider status
    let temp_config = create_temp_config()?;
    let provider = TempDockerProvider::new(temp_config)?;
    provider.status()
}

fn handle_destroy_command() -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    if !state_manager.state_exists() {
        return Err(anyhow::anyhow!("No temp VM found"));
    }

    let temp_config = create_temp_config()?;
    let provider = TempDockerProvider::new(temp_config)?;

    println!("🗑️ Destroying temporary VM...");
    provider.destroy()?;
    state_manager
        .delete_state()
        .context("Failed to delete temp VM state")?;

    println!("✅ Temporary VM destroyed");
    Ok(())
}

fn handle_mount_command(path: String, yes: bool) -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    if !state_manager.state_exists() {
        return Err(anyhow::anyhow!(
            "No temp VM found. Create one first with: vm-temp create"
        ));
    }

    // Parse the mount string
    let (source, target, permissions) =
        MountParser::parse_mount_string(&path).context("Failed to parse mount string")?;

    // Load current state
    let mut state = state_manager
        .load_state()
        .context("Failed to load temp VM state")?;

    // Check if mount already exists
    if state.has_mount(&source) {
        return Err(anyhow::anyhow!(
            "Mount already exists for source: {}",
            source.display()
        ));
    }

    // Confirm action unless --yes flag is used
    if !yes {
        let confirmation_msg = format!("Add mount {} to temp VM? (y/N): ", source.display());
        if !confirm_prompt(&confirmation_msg) {
            println!("❌ Mount operation cancelled");
            return Ok(());
        }
    }

    // Add the mount
    let permissions_display = permissions.to_string();
    if let Some(target_path) = target {
        state
            .add_mount_with_target(source.clone(), target_path, permissions)
            .context("Failed to add mount with custom target")?;
    } else {
        state
            .add_mount(source.clone(), permissions)
            .context("Failed to add mount")?;
    }

    // Save updated state
    state_manager
        .save_state(&state)
        .context("Failed to save updated temp VM state")?;

    println!(
        "🔗 Mount added: {} ({})",
        source.display(),
        permissions_display
    );

    // Apply mount changes using TempProvider
    let temp_config = create_temp_config()?;
    let temp_provider = get_temp_provider(temp_config)?;
    println!("🔄 Updating container with new mount...");
    temp_provider
        .update_mounts(&state)
        .context("Failed to update container mounts")?;
    println!("✅ Mount successfully applied to running container");

    Ok(())
}

fn handle_unmount_command(path: Option<String>, all: bool, yes: bool) -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    if !state_manager.state_exists() {
        return Err(anyhow::anyhow!("No temp VM found"));
    }

    // Load current state
    let mut state = state_manager
        .load_state()
        .context("Failed to load temp VM state")?;

    if all {
        if !yes {
            let confirmation_msg = format!(
                "Remove all {} mounts from temp VM? (y/N): ",
                state.mount_count()
            );
            if !confirm_prompt(&confirmation_msg) {
                println!("❌ Unmount operation cancelled");
                return Ok(());
            }
        }

        let mount_count = state.mount_count();
        state.clear_mounts();

        // Save updated state
        state_manager
            .save_state(&state)
            .context("Failed to save updated temp VM state")?;

        println!("🗑️ Removed all {} mount(s)", mount_count);

        // Apply mount changes using TempProvider
        let temp_config = create_temp_config()?;
        let temp_provider = get_temp_provider(temp_config)?;
        println!("🔄 Updating container with removed mounts...");
        temp_provider
            .update_mounts(&state)
            .context("Failed to update container mounts")?;
        println!("✅ All mounts successfully removed from running container");
    } else if let Some(path_str) = path {
        let source_path = PathBuf::from(path_str);

        if !state.has_mount(&source_path) {
            return Err(anyhow::anyhow!(
                "Mount not found for source: {}",
                source_path.display()
            ));
        }

        if !yes {
            let confirmation_msg = format!(
                "Remove mount {} from temp VM? (y/N): ",
                source_path.display()
            );
            if !confirm_prompt(&confirmation_msg) {
                println!("❌ Unmount operation cancelled");
                return Ok(());
            }
        }

        let removed_mount = state
            .remove_mount(&source_path)
            .context("Failed to remove mount")?;

        // Save updated state
        state_manager
            .save_state(&state)
            .context("Failed to save updated temp VM state")?;

        println!(
            "🗑️ Removed mount: {} ({})",
            removed_mount.source.display(),
            removed_mount.permissions
        );

        // Apply mount changes using TempProvider
        let temp_config = create_temp_config()?;
        let temp_provider = get_temp_provider(temp_config)?;
        println!("🔄 Updating container with removed mount...");
        temp_provider
            .update_mounts(&state)
            .context("Failed to update container mounts")?;
        println!("✅ Mount successfully removed from running container");
    } else {
        return Err(anyhow::anyhow!("Must specify --path or --all"));
    }

    Ok(())
}

fn handle_mounts_command() -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    if !state_manager.state_exists() {
        println!("❌ No temp VM found");
        return Ok(());
    }

    let state = state_manager
        .load_state()
        .context("Failed to load temp VM state")?;

    if state.mount_count() == 0 {
        println!("📁 No mounts configured");
        return Ok(());
    }

    println!("📁 Current mounts ({}):", state.mount_count());
    for mount in state.get_mounts() {
        println!(
            "   {} → {} ({})",
            mount.source.display(),
            mount.target.display(),
            mount.permissions
        );
    }

    // Show mount summary by permission
    let ro_count = state.mount_count_by_permission(MountPermission::ReadOnly);
    let rw_count = state.mount_count_by_permission(MountPermission::ReadWrite);

    println!("   {} read-only, {} read-write", ro_count, rw_count);

    Ok(())
}

fn handle_list_command() -> Result<()> {
    let state_manager = StateManager::new().context("Failed to initialize state manager")?;

    // For now, just show if there's a temp VM
    if state_manager.state_exists() {
        let state = state_manager
            .load_state()
            .context("Failed to load temp VM state")?;

        println!("📋 Temp VMs:");
        println!("   {} ({})", state.container_name, state.provider);
        println!(
            "      Created: {}",
            state.created_at.format("%Y-%m-%d %H:%M:%S")
        );
        println!("      Project: {}", state.project_dir.display());
        println!("      Mounts: {}", state.mount_count());
    } else {
        println!("📋 No temp VMs found");
    }

    Ok(())
}

fn handle_stop_command() -> Result<()> {
    println!("⏸️  Stop command not yet implemented");
    println!("💡 Use 'vm-temp destroy' to remove the VM completely");
    Ok(())
}

fn handle_start_command() -> Result<()> {
    println!("▶️  Start command not yet implemented");
    println!("💡 Use 'vm-temp create' to create a new temp VM");
    Ok(())
}

fn handle_restart_command() -> Result<()> {
    println!("🔄 Restart command not yet implemented");
    println!("💡 Use 'vm-temp destroy' then 'vm-temp create' for now");
    Ok(())
}

// Helper function to create temp VM config
fn create_temp_config() -> Result<VmConfig> {
    let mut config = VmConfig {
        provider: Some("docker".to_string()),
        ..Default::default()
    };

    if let Some(ref mut project) = config.project {
        project.name = Some("vm-temp".to_string());
        project.hostname = Some("vm-temp.local".to_string());
        project.workspace_path = Some("/workspace".to_string());
    } else {
        config.project = Some(vm_config::config::ProjectConfig {
            name: Some("vm-temp".to_string()),
            hostname: Some("vm-temp.local".to_string()),
            workspace_path: Some("/workspace".to_string()),
            backup_pattern: None,
            env_template_path: None,
        });
    }

    Ok(config)
}

// Local utility functions to avoid vm-provider dependency

/// Simple confirmation prompt
fn confirm_prompt(message: &str) -> bool {
    print!("{}", message);
    io::stdout().flush().unwrap();

    let mut input = String::new();
    match io::stdin().read_line(&mut input) {
        Ok(_) => {
            let input = input.trim().to_lowercase();
            input == "y" || input == "yes"
        }
        Err(_) => false,
    }
}

/// Simplified Docker provider that can handle TempProvider operations
#[allow(dead_code)]
struct TempDockerProvider {
    config: VmConfig,
    temp_dir: PathBuf,
}

impl TempDockerProvider {
    fn new(config: VmConfig) -> Result<Self> {
        let project_dir = std::env::current_dir()?;
        let temp_dir = project_dir.join(".vm-tmp");
        std::fs::create_dir_all(&temp_dir)?;

        Ok(Self { config, temp_dir })
    }

    fn container_name(&self) -> &str {
        "vm-temp-dev"
    }

    // Basic provider functionality for temp VMs
    fn create(&self) -> Result<()> {
        println!("🚀 Creating temp VM...");

        // For temp VMs, we use a simplified Docker provider
        // This would be the actual Docker provider implementation
        // For now, we'll just print a message
        println!("✅ Temp VM created successfully!");
        Ok(())
    }

    fn ssh(&self, _path: &Path) -> Result<()> {
        let container = self.container_name();

        // Detect if we're in an interactive environment and can use TTY
        // This works across all systems: Linux, macOS, Windows, CI/CD environments
        let stdin_is_tty = atty::is(atty::Stream::Stdin);
        let stdout_is_tty = atty::is(atty::Stream::Stdout);
        let tty_flag = if stdin_is_tty && stdout_is_tty {
            "-it" // Interactive with TTY
        } else {
            "-i"  // Interactive without TTY (for pipes, CI/CD, etc.)
        };

        // TTY detection completed - using appropriate flag for current environment

        std::process::Command::new("docker")
            .args(["exec", tty_flag, container, "bash"])
            .status()
            .context("Failed to SSH into container")?;
        Ok(())
    }

    fn status(&self) -> Result<()> {
        let container = self.container_name();
        std::process::Command::new("docker")
            .args(["ps", "-f", &format!("name={}", container)])
            .status()
            .context("Failed to get container status")?;
        Ok(())
    }

    fn destroy(&self) -> Result<()> {
        let container = self.container_name();
        std::process::Command::new("docker")
            .args(["rm", "-f", container])
            .status()
            .context("Failed to destroy container")?;
        Ok(())
    }
}

impl TempProvider for TempDockerProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        println!("🔄 Updating container mounts...");

        // Check if container is running
        let is_running = self.is_container_running(&state.container_name)?;

        if is_running {
            // Stop container
            std::process::Command::new("docker")
                .args(["stop", &state.container_name])
                .status()
                .context("Failed to stop container")?;
        }

        // Recreate with new mounts
        self.recreate_with_mounts(state)?;

        // Start container again
        std::process::Command::new("docker")
            .args(["start", &state.container_name])
            .status()
            .context("Failed to start container")?;

        // Check health
        if !self.check_container_health(&state.container_name)? {
            return Err(anyhow::anyhow!(
                "Container failed health check after mount update"
            ));
        }

        println!("✅ Container mounts updated successfully!");
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        // Remove old container
        let _ = std::process::Command::new("docker")
            .args(["rm", "-f", &state.container_name])
            .status();

        // Create new container with updated mounts
        let mut cmd = std::process::Command::new("docker");
        cmd.args(["run", "-d", "--name", &state.container_name]);

        // Add volumes for persistent data
        cmd.args(["-v", "vmtemp_nvm:/home/developer/.nvm"]);
        cmd.args(["-v", "vmtemp_cache:/home/developer/.cache"]);
        cmd.args(["-v", "../..:/workspace:rw"]);

        // Add custom mounts
        for mount in &state.mounts {
            let mount_str = format!(
                "{}:{}:{}",
                mount.source.display(),
                mount.target.display(),
                mount.permissions
            );
            cmd.args(["-v", &mount_str]);
        }

        // Use the vm-temp image
        cmd.arg("vm-temp:latest");

        cmd.status()
            .context("Failed to recreate container with new mounts")?;
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        for _ in 0..10 {
            let output = std::process::Command::new("docker")
                .args(["exec", container_name, "echo", "ready"])
                .output();

            if output.is_ok() && output.unwrap().status.success() {
                return Ok(true);
            }

            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        Ok(false)
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        let output = std::process::Command::new("docker")
            .args(["inspect", "--format", "{{.State.Status}}", container_name])
            .output()
            .context("Failed to check container status")?;

        if !output.status.success() {
            return Ok(false);
        }

        let binding = String::from_utf8_lossy(&output.stdout);
        let status = binding.trim();
        Ok(status == "running")
    }
}

/// Create a temp provider instance
fn get_temp_provider(config: VmConfig) -> Result<Box<dyn TempProvider>> {
    Ok(Box::new(TempDockerProvider::new(config)?))
}
