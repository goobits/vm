// Standard library
use std::io::{self, Write};
use std::path::PathBuf;

// External crates
use anyhow::{Context, Result};
use vm_common::{errors, vm_error};

// Internal imports
use crate::{MountParser, MountPermission, StateManager, TempVmState};
use vm_config::config::VmConfig;
use vm_provider::Provider;

/// Core temporary VM operations
pub struct TempVmOps;

impl TempVmOps {
    /// Create a new temporary VM with mounts
    pub fn create(
        mounts: Vec<String>,
        auto_destroy: bool,
        _config: VmConfig,
        provider: Box<dyn Provider>,
    ) -> Result<()> {
        let state_manager = StateManager::new().with_context(|| {
            "Failed to initialize temporary VM state manager. Check filesystem permissions"
        })?;

        // Parse mount strings using MountParser
        let parsed_mounts = MountParser::parse_mount_strings(&mounts).with_context(|| {
            "Failed to parse mount path specifications. Check mount string format"
        })?;

        // Get current project directory
        let project_dir = std::env::current_dir().with_context(|| {
            "Failed to get current working directory. Check directory permissions"
        })?;

        // Create temp VM state
        let mut temp_state = TempVmState::new(
            "vm-temp-dev".to_string(),
            provider.name().to_string(),
            project_dir,
            auto_destroy,
        );

        // Add all mounts to the state
        for (source, target, permissions) in parsed_mounts {
            if let Some(target_path) = target {
                let source_display = source.display().to_string();
                let target_display = target_path.display().to_string();
                temp_state
                    .add_mount_with_target(source, target_path, permissions)
                    .with_context(|| {
                        format!(
                            "Failed to add mount '{}' with custom target '{}'",
                            source_display, target_display
                        )
                    })?;
            } else {
                let source_display = source.display().to_string();
                temp_state.add_mount(source, permissions).with_context(|| {
                    format!("Failed to add mount for path '{}'", source_display)
                })?;
            }
        }

        // Create the VM using the provided provider
        if let Some(_temp_provider) = provider.as_temp_provider() {
            provider.create()?;
        } else {
            return Err(errors::temp::provider_unsupported());
        }

        // Save state
        state_manager
            .save_state(&temp_state)
            .with_context(|| "Failed to save temporary VM state to disk. Check filesystem permissions and available space")?;

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
                .with_context(|| "Failed to delete temporary VM state from disk. State file may be in use or filesystem is read-only")?;
        } else {
            println!("💡 Use 'vm temp ssh' to connect");
            println!("   Use 'vm temp destroy' when done");
        }

        Ok(())
    }

    /// SSH into the temporary VM
    pub fn ssh(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

        if !state_manager.state_exists() {
            return Err(errors::temp::temp_vm_not_found());
        }

        provider.ssh(&PathBuf::from("."))
    }

    /// Show temporary VM status
    pub fn status(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for status check")?;

        if !state_manager.state_exists() {
            vm_error!("No temp VM found");
            println!("💡 Create one with: vm temp create ./your-directory");
            return Ok(());
        }

        let state = state_manager.load_state().with_context(|| {
            "Failed to load temporary VM state from disk. State file may be corrupted"
        })?;

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
        provider.status()
    }

    /// Destroy the temporary VM
    pub fn destroy(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for VM destruction")?;

        if !state_manager.state_exists() {
            // Use the new error function, which already provides a user-friendly
            // message and returns an anyhow::Error.
            return Err(errors::config::config_not_found(
                state_manager.state_file_path(),
            ));
        }

        println!("🗑️ Destroying temporary VM...");
        provider.destroy()?;

        state_manager
            .delete_state()
            .with_context(|| "Failed to delete temporary VM state from disk. State file may be in use or filesystem is read-only")?;

        println!("\n✅ Temporary VM destroyed");
        println!("\n💡 Create a new one: vm temp create <directory>");
        Ok(())
    }

    /// Add mount to running temporary VM
    pub fn mount(path: String, yes: bool, provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for mount operation")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!(
                "No temp VM found. Create one first with: vm temp create"
            ));
        }

        // Parse the mount string
        let (source, target, permissions) =
            MountParser::parse_mount_string(&path).with_context(|| {
                format!(
                    "Failed to parse mount string '{}'. Check mount path format",
                    path
                )
            })?;

        // Load current state
        let mut state = state_manager
            .load_state()
            .with_context(|| "Failed to load temporary VM state from disk for mount operation. State file may be corrupted")?;

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
            if !Self::confirm_prompt(&confirmation_msg) {
                vm_error!("Mount operation cancelled");
                return Ok(());
            }
        }

        // Add the mount
        let permissions_display = permissions.to_string();
        let target_clone = target.clone();
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
        if let Some(temp_provider) = provider.as_temp_provider() {
            println!("🔄 Updating container with new mount...");
            temp_provider
                .update_mounts(&state)
                .context("Failed to update container mounts")?;
            println!("\n✅ Mount successfully applied");
            println!("  Source: {}", source.display());
            if let Some(target_path) = &target_clone {
                println!("  Target: {}", target_path.display());
            }
            println!("  Access: {}\n", permissions_display);
            println!("💡 View all mounts: vm temp mounts");
        } else {
            return Err(anyhow::anyhow!("Provider does not support mount updates"));
        }

        Ok(())
    }

    /// Remove mount from temporary VM
    pub fn unmount(
        path: Option<String>,
        all: bool,
        yes: bool,
        provider: Box<dyn Provider>,
    ) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

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
                if !Self::confirm_prompt(&confirmation_msg) {
                    vm_error!("Unmount operation cancelled");
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
            if let Some(temp_provider) = provider.as_temp_provider() {
                println!("🔄 Updating container with removed mounts...");
                temp_provider
                    .update_mounts(&state)
                    .context("Failed to update container mounts")?;
                println!("\n✅ All mounts removed ({})", mount_count);
                println!("\n💡 Add new mounts: vm temp mount <source>:<target>");
            }
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
                if !Self::confirm_prompt(&confirmation_msg) {
                    vm_error!("Unmount operation cancelled");
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
            if let Some(temp_provider) = provider.as_temp_provider() {
                println!("🔄 Updating container with removed mount...");
                temp_provider
                    .update_mounts(&state)
                    .context("Failed to update container mounts")?;
                println!("\n✅ Mount removed");
                println!("  Path: {}\n", source_path.display());
                println!("💡 View remaining mounts: vm temp mounts");
            }
        } else {
            println!("❌ Must specify what to unmount\n");
            println!("💡 Options:");
            println!("  • Unmount specific: vm temp unmount --path <path>");
            println!("  • Unmount all: vm temp unmount --all");
            return Err(anyhow::anyhow!("Must specify --path or --all"));
        }

        Ok(())
    }

    /// List current mounts
    pub fn mounts() -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

        if !state_manager.state_exists() {
            println!("🔍 No temp VM found\n");
            println!("💡 Create one with: vm temp create <directory>");
            return Ok(());
        }

        let state = state_manager
            .load_state()
            .context("Failed to load temp VM state")?;

        if state.mount_count() == 0 {
            println!("📁 No mounts configured\n");
            println!("💡 Add a mount: vm temp mount <source>:<target>");
            return Ok(());
        }

        println!("📁 Current mounts ({})", state.mount_count());
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

    /// List all temporary VMs
    pub fn list() -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

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
            println!("📋 No temp VMs found\n");
            println!("💡 Create one: vm temp create <directory>");
        }

        Ok(())
    }

    /// Stop temporary VM
    pub fn stop(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!(
                "No temp VM found. Create one with: vm temp create <directory>"
            ));
        }

        println!("🛑 Stopping temporary VM...");

        match provider.stop() {
            Ok(()) => {
                println!("\n✅ Temporary VM stopped");
                println!("\n💡 Restart with: vm temp start");
                Ok(())
            }
            Err(e) => {
                println!("\n❌ Failed to stop temporary VM");
                println!("   Error: {}", e);
                Err(e)
            }
        }
    }

    /// Start temporary VM
    pub fn start(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!(
                "No temp VM found. Create one with: vm temp create <directory>"
            ));
        }

        let state = state_manager
            .load_state()
            .context("Failed to load temp VM state")?;

        println!("🚀 Starting temporary VM...");

        match provider.start() {
            Ok(()) => {
                println!("\n✅ Temporary VM started");

                // Show mount info if any
                if state.mount_count() > 0 {
                    println!("  Mounts:     {} configured", state.mount_count());
                }

                println!("\n💡 Connect with: vm temp ssh");
                Ok(())
            }
            Err(e) => {
                println!("\n❌ Failed to start temporary VM");
                println!("   Error: {}", e);
                println!("\n💡 Try: vm temp destroy && vm temp create <directory>");
                Err(e)
            }
        }
    }

    /// Restart temporary VM
    pub fn restart(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!(
                "No temp VM found. Create one with: vm temp create <directory>"
            ));
        }

        let state = state_manager
            .load_state()
            .context("Failed to load temp VM state")?;

        println!("🔄 Restarting temporary VM...");
        println!("  ✓ Stopping container");
        println!("  ✓ Starting container");

        match provider.restart() {
            Ok(()) => {
                println!("  ✓ Services ready\n");
                println!("✅ Temporary VM restarted");

                if state.mount_count() > 0 {
                    println!("  Mounts:     {} active", state.mount_count());
                }

                println!("\n💡 Connect with: vm temp ssh");
                Ok(())
            }
            Err(e) => {
                println!("\n❌ Failed to restart temporary VM");
                println!("   Error: {}", e);
                Err(e)
            }
        }
    }

    // Helper functions

    /// Simple confirmation prompt
    fn confirm_prompt(message: &str) -> bool {
        print!("{}", message);
        // If stdout flush fails, continue anyway - the prompt might still work
        let _ = io::stdout().flush();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let input = input.trim().to_lowercase();
                input == "y" || input == "yes"
            }
            Err(_) => false,
        }
    }
}
