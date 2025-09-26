// Standard library
use std::io::{self, Write};
use std::path::PathBuf;

// External crates
use anyhow::{Context, Result};
use vm_common::{
    errors,
    messages::{messages::MESSAGES, msg},
    vm_error, vm_println,
};

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

        vm_println!(
            "{}",
            msg!(
                MESSAGES.temp_vm_created_with_mounts,
                count = temp_state.mount_count().to_string()
            )
        );

        if auto_destroy {
            // SSH then destroy
            vm_println!("{}", MESSAGES.temp_vm_connecting);
            provider.ssh(None, &PathBuf::from("."))?;
            vm_println!("{}", MESSAGES.temp_vm_auto_destroying);
            provider.destroy(None)?;
            state_manager
                .delete_state()
                .with_context(|| "Failed to delete temporary VM state from disk. State file may be in use or filesystem is read-only")?;
        } else {
            vm_println!("{}", MESSAGES.temp_vm_usage_hint);
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

        provider.ssh(None, &PathBuf::from("."))
    }

    /// Show temporary VM status
    pub fn status(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for status check")?;

        if !state_manager.state_exists() {
            vm_println!("{}", MESSAGES.temp_vm_no_vm_found);
            vm_println!("{}", MESSAGES.temp_vm_create_hint);
            return Ok(());
        }

        let state = state_manager.load_state().with_context(|| {
            "Failed to load temporary VM state from disk. State file may be corrupted"
        })?;

        vm_println!("{}", MESSAGES.temp_vm_status);
        vm_println!(
            "{}",
            msg!(
                MESSAGES.temp_vm_container_info,
                name = &state.container_name
            )
        );
        vm_println!(
            "{}",
            msg!(MESSAGES.temp_vm_provider_info, provider = &state.provider)
        );
        vm_println!(
            "   Created: {}",
            state.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        vm_println!(
            "{}",
            msg!(
                MESSAGES.temp_vm_project_info,
                path = state.project_dir.display().to_string()
            )
        );
        vm_println!(
            "{}",
            msg!(
                MESSAGES.temp_vm_mounts_info,
                count = state.mount_count().to_string()
            )
        );

        if state.is_auto_destroy() {
            vm_println!("{}", MESSAGES.temp_vm_auto_destroy_enabled);
        }

        // Check provider status
        provider.status(None)
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

        vm_println!("{}", MESSAGES.temp_vm_destroying);
        provider.destroy(None)?;

        state_manager
            .delete_state()
            .with_context(|| "Failed to delete temporary VM state from disk. State file may be in use or filesystem is read-only")?;

        vm_println!("{}", MESSAGES.temp_vm_destroyed);
        vm_println!("{}", MESSAGES.temp_vm_create_hint);
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

        vm_println!(
            "🔗 Mount added: {} ({})",
            source.display(),
            permissions_display
        );

        // Apply mount changes using TempProvider
        if let Some(temp_provider) = provider.as_temp_provider() {
            vm_println!("{}", MESSAGES.temp_vm_updating_container);
            temp_provider
                .update_mounts(&state)
                .context("Failed to update container mounts")?;
            vm_println!("{}", MESSAGES.temp_vm_mount_applied);
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_mount_source,
                    source = source.display().to_string()
                )
            );
            if let Some(target_path) = &target_clone {
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.temp_vm_mount_target,
                        target = target_path.display().to_string()
                    )
                );
            }
            vm_println!(
                "{}",
                msg!(MESSAGES.temp_vm_mount_access, access = permissions_display)
            );
            vm_println!("{}", MESSAGES.temp_vm_view_mounts_hint);
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

            vm_println!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_mounts_removed,
                    count = mount_count.to_string()
                )
            );

            // Apply mount changes using TempProvider
            if let Some(temp_provider) = provider.as_temp_provider() {
                vm_println!("{}", MESSAGES.temp_vm_updating_container);
                temp_provider
                    .update_mounts(&state)
                    .context("Failed to update container mounts")?;
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.temp_vm_all_mounts_removed,
                        count = mount_count.to_string()
                    )
                );
                vm_println!("{}", MESSAGES.temp_vm_add_mounts_hint);
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
                vm_println!("{}", MESSAGES.temp_vm_updating_container);
                temp_provider
                    .update_mounts(&state)
                    .context("Failed to update container mounts")?;
                vm_println!("{}", MESSAGES.temp_vm_mount_removed);
                vm_println!("  Path: {}", source_path.display());
                vm_println!("{}", MESSAGES.temp_vm_view_remaining_hint);
            }
        } else {
            vm_println!("{}", MESSAGES.temp_vm_unmount_required);
            vm_println!("{}", MESSAGES.temp_vm_unmount_options);
            vm_println!("{}", MESSAGES.temp_vm_unmount_specific);
            vm_println!("{}", MESSAGES.temp_vm_unmount_all);
            return Err(anyhow::anyhow!("Must specify --path or --all"));
        }

        Ok(())
    }

    /// List current mounts
    pub fn mounts() -> Result<()> {
        let state_manager = StateManager::new()
            .with_context(|| "Failed to initialize state manager for SSH connection")?;

        if !state_manager.state_exists() {
            vm_println!("{}", MESSAGES.temp_vm_no_vm_found);
            vm_println!("{}", MESSAGES.temp_vm_create_hint);
            return Ok(());
        }

        let state = state_manager
            .load_state()
            .context("Failed to load temp VM state")?;

        if state.mount_count() == 0 {
            vm_println!("{}", MESSAGES.temp_vm_no_mounts);
            vm_println!("{}", MESSAGES.temp_vm_add_mount_hint);
            return Ok(());
        }

        vm_println!(
            "{}",
            msg!(
                MESSAGES.temp_vm_current_mounts,
                count = state.mount_count().to_string()
            )
        );
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
        vm_println!(
            "{}",
            msg!(
                MESSAGES.temp_vm_mount_summary,
                ro_count = ro_count.to_string(),
                rw_count = rw_count.to_string()
            )
        );

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

            vm_println!("{}", MESSAGES.temp_vm_list_header);
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_list_item,
                    name = &state.container_name,
                    provider = &state.provider
                )
            );
            println!(
                "      Created: {}",
                state.created_at.format("%Y-%m-%d %H:%M:%S")
            );
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_list_project,
                    path = state.project_dir.display().to_string()
                )
            );
            vm_println!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_list_mounts,
                    count = state.mount_count().to_string()
                )
            );
        } else {
            vm_println!("{}", MESSAGES.temp_vm_list_empty);
            vm_println!("{}", MESSAGES.temp_vm_list_create_hint);
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

        vm_println!("{}", MESSAGES.temp_vm_stopping);

        match provider.stop(None) {
            Ok(()) => {
                vm_println!("{}", MESSAGES.temp_vm_stopped_success);
                vm_println!("{}", MESSAGES.temp_vm_restart_hint);
                Ok(())
            }
            Err(e) => {
                vm_println!("{}", MESSAGES.temp_vm_failed_to_stop);
                vm_println!("   Error: {}", e);
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

        vm_println!("{}", MESSAGES.temp_vm_starting);

        match provider.start(None) {
            Ok(()) => {
                vm_println!("{}", MESSAGES.temp_vm_started_success);

                // Show mount info if any
                if state.mount_count() > 0 {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.temp_vm_mounts_configured,
                            count = state.mount_count().to_string()
                        )
                    );
                }

                vm_println!("{}", MESSAGES.temp_vm_connect_hint);
                Ok(())
            }
            Err(e) => {
                vm_println!("{}", MESSAGES.temp_vm_failed_to_start);
                vm_println!("   {}", msg!(MESSAGES.error_generic, error = e.to_string()));
                vm_println!("\n💡 Try: vm temp destroy && vm temp create <directory>");
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

        vm_println!("{}", MESSAGES.temp_vm_restarting);
        vm_println!("{}", MESSAGES.temp_vm_stopping_step);
        vm_println!("{}", MESSAGES.temp_vm_starting_step);

        match provider.restart(None) {
            Ok(()) => {
                vm_println!("{}", MESSAGES.temp_vm_services_ready);
                vm_println!("{}", MESSAGES.temp_vm_restarted_success);

                if state.mount_count() > 0 {
                    vm_println!(
                        "{}",
                        msg!(
                            MESSAGES.temp_vm_mounts_active,
                            count = state.mount_count().to_string()
                        )
                    );
                }

                vm_println!("{}", MESSAGES.temp_vm_connect_hint);
                Ok(())
            }
            Err(e) => {
                vm_println!("{}", MESSAGES.temp_vm_failed_to_restart);
                vm_println!("   Error: {}", e);
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
