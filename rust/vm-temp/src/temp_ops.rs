// Standard library
use std::io::{self, Write};
use std::path::PathBuf;

// External crates
use anyhow::{Context, Result};
use vm_common::{vm_error, vm_success};

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
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        // Parse mount strings using MountParser
        let parsed_mounts =
            MountParser::parse_mount_strings(&mounts).context("Failed to parse mount strings")?;

        // Get current project directory
        let project_dir = std::env::current_dir().context("Failed to get current directory")?;

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
                temp_state
                    .add_mount_with_target(source, target_path, permissions)
                    .context("Failed to add mount with custom target")?;
            } else {
                temp_state
                    .add_mount(source, permissions)
                    .context("Failed to add mount")?;
            }
        }

        // Create the VM using the provided provider
        if let Some(_temp_provider) = provider.as_temp_provider() {
            provider.create()?;
        } else {
            return Err(anyhow::anyhow!("Provider does not support temporary VMs"));
        }

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
            println!("💡 Use 'vm temp ssh' to connect");
            println!("   Use 'vm temp destroy' when done");
        }

        Ok(())
    }

    /// SSH into the temporary VM
    pub fn ssh(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!(
                "No temp VM found\n💡 Create one with: vm temp create ./your-directory"
            ));
        }

        provider.ssh(&PathBuf::from("."))
    }

    /// Show temporary VM status
    pub fn status(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            vm_error!("No temp VM found");
            println!("💡 Create one with: vm temp create ./your-directory");
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
        provider.status()
    }

    /// Destroy the temporary VM
    pub fn destroy(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!("No temp VM found"));
        }

        println!("🗑️ Destroying temporary VM...");
        provider.destroy()?;

        state_manager
            .delete_state()
            .context("Failed to delete temp VM state")?;

        vm_success!("Temporary VM destroyed");
        Ok(())
    }

    /// Add mount to running temporary VM
    pub fn mount(path: String, yes: bool, provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!(
                "No temp VM found. Create one first with: vm temp create"
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
            if !Self::confirm_prompt(&confirmation_msg) {
                vm_error!("Mount operation cancelled");
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
        if let Some(temp_provider) = provider.as_temp_provider() {
            println!("🔄 Updating container with new mount...");
            temp_provider
                .update_mounts(&state)
                .context("Failed to update container mounts")?;
            vm_success!("Mount successfully applied to running container");
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
                vm_success!("All mounts successfully removed from running container");
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
                vm_success!("Mount successfully removed from running container");
            }
        } else {
            return Err(anyhow::anyhow!("Must specify --path or --all"));
        }

        Ok(())
    }

    /// List current mounts
    pub fn mounts() -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            vm_error!("No temp VM found");
            return Ok(());
        }

        let state = state_manager
            .load_state()
            .context("Failed to load temp VM state")?;

        if state.mount_count() == 0 {
            println!("📁 No mounts configured");
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

    /// Stop temporary VM
    pub fn stop(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!("No temp VM found"));
        }

        println!("⏸️ Stopping temporary VM...");
        provider.stop()?;
        vm_success!("Temporary VM stopped");

        Ok(())
    }

    /// Start temporary VM
    pub fn start(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!("No temp VM found"));
        }

        println!("▶️ Starting temporary VM...");
        provider.start()?;
        vm_success!("Temporary VM started");

        Ok(())
    }

    /// Restart temporary VM
    pub fn restart(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().context("Failed to initialize state manager")?;

        if !state_manager.state_exists() {
            return Err(anyhow::anyhow!("No temp VM found"));
        }

        println!("🔄 Restarting temporary VM...");
        provider.restart()?;
        vm_success!("Temporary VM restarted");

        Ok(())
    }

    // Helper functions

    /// Simple confirmation prompt
    fn confirm_prompt(message: &str) -> bool {
        print!("{}", message);
        io::stdout().flush().expect("Failed to flush stdout");

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
