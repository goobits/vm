// Standard library
use std::io::{self, Write};
use std::path::PathBuf;

// External crates
use tracing::{error, info};
use vm_cli::msg;
use vm_core::error::{Result, VmError};
use vm_messages::messages::MESSAGES;

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
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize temporary VM state manager. Check filesystem permissions: {e}"
            ))
        })?;

        // Parse mount strings using MountParser
        let parsed_mounts = MountParser::parse_mount_strings(&mounts).map_err(|e| {
            VmError::Config(format!(
                "Failed to parse mount path specifications. Check mount string format: {e}"
            ))
        })?;

        // Get current project directory
        let project_dir = std::env::current_dir().map_err(|e| {
            VmError::Filesystem(format!(
                "Failed to get current working directory. Check directory permissions: {e}"
            ))
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
                    .map_err(|e| {
                        VmError::Config(format!(
                            "Failed to add mount '{source_display}' with custom target '{target_display}': {e}"
                        ))
                    })?;
            } else {
                let source_display = source.display().to_string();
                temp_state.add_mount(source, permissions).map_err(|e| {
                    VmError::Config(format!(
                        "Failed to add mount for path '{source_display}': {e}"
                    ))
                })?;
            }
        }

        // Create the VM using the provided provider
        if let Some(_temp_provider) = provider.as_temp_provider() {
            provider.create()?;
        } else {
            return Err(VmError::Internal(
                "Provider does not support temp VM operations".to_string(),
            ));
        }

        // Save state
        state_manager.save_state(&temp_state)?;

        info!(
            "{}",
            msg!(
                MESSAGES.temp_vm_created_with_mounts,
                count = temp_state.mount_count().to_string()
            )
        );

        if auto_destroy {
            // SSH then destroy
            info!("{}", MESSAGES.temp_vm_connecting);
            provider.ssh(None, &PathBuf::from("."))?;
            info!("{}", MESSAGES.temp_vm_auto_destroying);
            provider.destroy(None)?;
            state_manager.delete_state()?;
        } else {
            info!("{}", MESSAGES.temp_vm_usage_hint);
        }

        Ok(())
    }

    /// SSH into the temporary VM
    pub fn ssh(provider: Box<dyn Provider>, config: VmConfig) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {}",
                e
            ))
        })?;

        if !state_manager.state_exists() {
            // Prompt user to create temp VM
            if Self::prompt_for_temp_vm_creation("now") {
                info!("\n🚀 Creating temporary VM...");

                // Create temp VM with current directory as mount
                let project_dir = std::env::current_dir().map_err(|e| {
                    VmError::Filesystem(format!("Failed to get current directory: {}", e))
                })?;

                let mounts = vec![project_dir.display().to_string()];
                Self::create(mounts, false, config, provider.clone())?;

                info!("Connecting to temporary VM...");
            // Fall through to SSH connection below
            } else {
                info!("Cancelled. Create a temp VM with: vm temp create <directory>");
                return Ok(());
            }
        }

        provider.ssh(None, &PathBuf::from("."))
    }

    /// Show temporary VM status
    pub fn status(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for status check: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            info!("{}", MESSAGES.temp_vm_no_vm_found);
            info!("{}", MESSAGES.temp_vm_create_hint);
            return Ok(());
        }

        let state = state_manager.load_state()?;

        info!("{}", MESSAGES.temp_vm_status);
        info!(
            "{}",
            msg!(
                MESSAGES.temp_vm_container_info,
                name = &state.container_name
            )
        );
        info!(
            "{}",
            msg!(MESSAGES.temp_vm_provider_info, provider = &state.provider)
        );
        info!(
            "   Created: {}",
            state.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        info!(
            "{}",
            msg!(
                MESSAGES.temp_vm_project_info,
                path = state.project_dir.display().to_string()
            )
        );
        info!(
            "{}",
            msg!(
                MESSAGES.temp_vm_mounts_info,
                count = state.mount_count().to_string()
            )
        );

        if state.is_auto_destroy() {
            info!("{}", MESSAGES.temp_vm_auto_destroy_enabled);
        }

        // Check provider status
        provider.status(None)
    }

    /// Destroy the temporary VM
    pub fn destroy(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for VM destruction: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            // Use the new error function, which already provides a user-friendly
            // message and returns a VmError.
            return Err(VmError::Internal(format!(
                "Config not found at: {}",
                state_manager.state_file_path().display()
            )));
        }

        info!("{}", MESSAGES.temp_vm_destroying);
        provider.destroy(None)?;

        state_manager.delete_state()?;

        info!("{}", MESSAGES.temp_vm_destroyed);
        info!("{}", MESSAGES.temp_vm_create_hint);
        Ok(())
    }

    /// Add mount to running temporary VM
    pub fn mount(
        path: String,
        yes: bool,
        provider: Box<dyn Provider>,
        config: VmConfig,
    ) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for mount operation: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            // Prompt user to create temp VM with this mount
            if Self::prompt_for_temp_vm_creation("with this mount") {
                info!("\n🚀 Creating temporary VM...");

                // Create temp VM with the requested mount
                Self::create(vec![path.clone()], false, config, provider.clone())?;

                info!("💡 Tip: Connect with 'vm temp ssh'");
                return Ok(());
            } else {
                info!("Cancelled. Create a temp VM with: vm temp create <directory>");
                return Ok(());
            }
        }

        // Parse the mount string
        let (source, target, permissions) =
            MountParser::parse_mount_string(&path).map_err(|e| {
                VmError::Config(format!(
                    "Failed to parse mount string '{path}'. Check mount path format: {e}"
                ))
            })?;

        // Load current state
        let mut state = state_manager.load_state()?;

        // Check if mount already exists
        if state.has_mount(&source) {
            return Err(VmError::Internal(format!(
                "Mount already exists for source: {}",
                source.display()
            )));
        }

        // Confirm action unless --yes flag is used
        if !yes {
            let confirmation_msg = msg!(
                MESSAGES.temp_vm_confirm_add_mount,
                source = source.display().to_string()
            );
            if !Self::confirm_prompt(&confirmation_msg) {
                error!("Mount operation cancelled");
                return Ok(());
            }
        }

        // Add the mount
        let permissions_display = permissions.to_string();
        let target_clone = target.clone();
        if let Some(target_path) = target {
            state
                .add_mount_with_target(source.clone(), target_path, permissions)
                .map_err(|e| {
                    VmError::Config(format!("Failed to add mount with custom target: {e}"))
                })?;
        } else {
            state
                .add_mount(source.clone(), permissions)
                .map_err(|e| VmError::Config(format!("Failed to add mount: {e}")))?;
        }

        // Save updated state
        state_manager.save_state(&state)?;

        info!(
            "🔗 Mount added: {} ({})",
            source.display(),
            permissions_display
        );

        // Apply mount changes using TempProvider
        if let Some(temp_provider) = provider.as_temp_provider() {
            info!("{}", MESSAGES.temp_vm_updating_container);
            temp_provider.update_mounts(&state).map_err(|e| {
                VmError::Provider(format!("Failed to update container mounts: {e}"))
            })?;
            info!("{}", MESSAGES.temp_vm_mount_applied);
            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_mount_source,
                    source = source.display().to_string()
                )
            );
            if let Some(target_path) = &target_clone {
                info!(
                    "{}",
                    msg!(
                        MESSAGES.temp_vm_mount_target,
                        target = target_path.display().to_string()
                    )
                );
            }
            info!(
                "{}",
                msg!(MESSAGES.temp_vm_mount_access, access = permissions_display)
            );
            info!("{}", MESSAGES.temp_vm_view_mounts_hint);
        } else {
            return Err(VmError::Internal(
                "Provider does not support mount updates".to_string(),
            ));
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
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            info!("No temporary VM found.");
            info!("💡 Create one with: vm temp create <directory>");
            info!("   Or use 'vm temp ssh' to create and connect automatically");
            return Err(VmError::NotFound("No temporary VM exists".to_string()));
        }

        // Load current state
        let mut state = state_manager.load_state()?;

        if all {
            if !yes {
                let confirmation_msg = msg!(
                    MESSAGES.temp_vm_confirm_remove_all_mounts,
                    count = state.mount_count().to_string()
                );
                if !Self::confirm_prompt(&confirmation_msg) {
                    error!("Unmount operation cancelled");
                    return Ok(());
                }
            }

            let mount_count = state.mount_count();
            state.clear_mounts();

            // Save updated state
            state_manager.save_state(&state).map_err(|e| {
                VmError::Internal(format!("Failed to save updated temp VM state: {e}"))
            })?;

            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_mounts_removed,
                    count = mount_count.to_string()
                )
            );

            // Apply mount changes using TempProvider
            if let Some(temp_provider) = provider.as_temp_provider() {
                info!("{}", MESSAGES.temp_vm_updating_container);
                temp_provider.update_mounts(&state).map_err(|e| {
                    VmError::Provider(format!("Failed to update container mounts: {e}"))
                })?;
                info!(
                    "{}",
                    msg!(
                        MESSAGES.temp_vm_all_mounts_removed,
                        count = mount_count.to_string()
                    )
                );
                info!("{}", MESSAGES.temp_vm_add_mounts_hint);
            }
        } else if let Some(path_str) = path {
            let source_path = PathBuf::from(path_str);

            if !state.has_mount(&source_path) {
                return Err(VmError::Internal(format!(
                    "Mount not found for source: {}",
                    source_path.display()
                )));
            }

            if !yes {
                let confirmation_msg = msg!(
                    MESSAGES.temp_vm_confirm_remove_mount,
                    source = source_path.display().to_string()
                );
                if !Self::confirm_prompt(&confirmation_msg) {
                    error!("Unmount operation cancelled");
                    return Ok(());
                }
            }

            let removed_mount = state
                .remove_mount(&source_path)
                .map_err(|e| VmError::Config(format!("Failed to remove mount: {e}")))?;

            // Save updated state
            state_manager.save_state(&state).map_err(|e| {
                VmError::Internal(format!("Failed to save updated temp VM state: {e}"))
            })?;

            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_mount_removed_detail,
                    source = removed_mount.source.display().to_string(),
                    permissions = removed_mount.permissions.to_string()
                )
            );

            // Apply mount changes using TempProvider
            if let Some(temp_provider) = provider.as_temp_provider() {
                info!("{}", MESSAGES.temp_vm_updating_container);
                temp_provider.update_mounts(&state).map_err(|e| {
                    VmError::Provider(format!("Failed to update container mounts: {e}"))
                })?;
                info!("{}", MESSAGES.temp_vm_mount_removed);
                info!("  Path: {}", source_path.display());
                info!("{}", MESSAGES.temp_vm_view_remaining_hint);
            }
        } else {
            info!("{}", MESSAGES.temp_vm_unmount_required);
            info!("{}", MESSAGES.temp_vm_unmount_options);
            info!("{}", MESSAGES.temp_vm_unmount_specific);
            info!("{}", MESSAGES.temp_vm_unmount_all);
            return Err(VmError::Internal(
                "Must specify --path or --all".to_string(),
            ));
        }

        Ok(())
    }

    /// List current mounts
    pub fn mounts() -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            info!("{}", MESSAGES.temp_vm_no_vm_found);
            info!("{}", MESSAGES.temp_vm_create_hint);
            return Ok(());
        }

        let state = state_manager.load_state()?;

        if state.mount_count() == 0 {
            info!("{}", MESSAGES.temp_vm_no_mounts);
            info!("{}", MESSAGES.temp_vm_add_mount_hint);
            return Ok(());
        }

        info!(
            "{}",
            msg!(
                MESSAGES.temp_vm_current_mounts,
                count = state.mount_count().to_string()
            )
        );
        for mount in state.get_mounts() {
            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_mount_display_item,
                    source = mount.source.display().to_string(),
                    target = mount.target.display().to_string(),
                    permissions = mount.permissions.to_string()
                )
            );
        }

        // Show mount summary by permission
        let ro_count = state.mount_count_by_permission(MountPermission::ReadOnly);
        let rw_count = state.mount_count_by_permission(MountPermission::ReadWrite);
        info!(
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
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {e}"
            ))
        })?;

        // For now, just show if there's a temp VM
        if state_manager.state_exists() {
            let state = state_manager
                .load_state()
                .map_err(|e| VmError::Internal(format!("Failed to load temp VM state: {e}")))?;

            info!("{}", MESSAGES.temp_vm_list_header);
            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_list_item,
                    name = &state.container_name,
                    provider = &state.provider
                )
            );
            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_list_created_date,
                    date = state.created_at.format("%Y-%m-%d %H:%M:%S").to_string()
                )
            );
            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_list_project,
                    path = state.project_dir.display().to_string()
                )
            );
            info!(
                "{}",
                msg!(
                    MESSAGES.temp_vm_list_mounts,
                    count = state.mount_count().to_string()
                )
            );
        } else {
            info!("{}", MESSAGES.temp_vm_list_empty);
            info!("{}", MESSAGES.temp_vm_list_create_hint);
        }

        Ok(())
    }

    /// Stop temporary VM
    pub fn stop(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            info!("No temporary VM found.");
            info!("💡 Create one with: vm temp create <directory>");
            info!("   Or use 'vm temp ssh' to create and connect automatically");
            return Err(VmError::NotFound("No temporary VM exists".to_string()));
        }

        info!("{}", MESSAGES.temp_vm_stopping);

        match provider.stop(None) {
            Ok(()) => {
                info!("{}", MESSAGES.temp_vm_stopped_success);
                info!("{}", MESSAGES.temp_vm_restart_hint);
                Ok(())
            }
            Err(e) => {
                error!("{}", MESSAGES.temp_vm_failed_to_stop);
                error!("   Error: {}", e);
                Err(e)
            }
        }
    }

    /// Start temporary VM
    pub fn start(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            info!("No temporary VM found.");
            info!("💡 Create one with: vm temp create <directory>");
            info!("   Or use 'vm temp ssh' to create and connect automatically");
            return Err(VmError::NotFound("No temporary VM exists".to_string()));
        }

        let state = state_manager.load_state()?;

        info!("{}", MESSAGES.temp_vm_starting);

        match provider.start(None) {
            Ok(()) => {
                info!("{}", MESSAGES.temp_vm_started_success);

                // Show mount info if any
                if state.mount_count() > 0 {
                    info!(
                        "{}",
                        msg!(
                            MESSAGES.temp_vm_mounts_configured,
                            count = state.mount_count().to_string()
                        )
                    );
                }

                info!("{}", MESSAGES.temp_vm_connect_hint);
                Ok(())
            }
            Err(e) => {
                error!("{}", MESSAGES.temp_vm_failed_to_start);
                error!("   {}", msg!(MESSAGES.error_generic, error = e.to_string()));
                info!("\n💡 Try: vm temp destroy && vm temp create <directory>");
                Err(e)
            }
        }
    }

    /// Restart temporary VM
    pub fn restart(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {e}"
            ))
        })?;

        if !state_manager.state_exists() {
            info!("No temporary VM found.");
            info!("💡 Create one with: vm temp create <directory>");
            info!("   Or use 'vm temp ssh' to create and connect automatically");
            return Err(VmError::NotFound("No temporary VM exists".to_string()));
        }

        let state = state_manager.load_state()?;

        info!("{}", MESSAGES.temp_vm_restarting);
        info!("{}", MESSAGES.temp_vm_stopping_step);
        info!("{}", MESSAGES.temp_vm_starting_step);

        match provider.restart(None) {
            Ok(()) => {
                info!("{}", MESSAGES.temp_vm_services_ready);
                info!("{}", MESSAGES.temp_vm_restarted_success);

                if state.mount_count() > 0 {
                    info!(
                        "{}",
                        msg!(
                            MESSAGES.temp_vm_mounts_active,
                            count = state.mount_count().to_string()
                        )
                    );
                }

                info!("{}", MESSAGES.temp_vm_connect_hint);
                Ok(())
            }
            Err(e) => {
                error!("{}", MESSAGES.temp_vm_failed_to_restart);
                error!("   Error: {}", e);
                Err(e)
            }
        }
    }

    // Helper functions

    /// Helper function to prompt for temp VM creation
    /// Returns true if user wants to create, false otherwise
    fn prompt_for_temp_vm_creation(action_context: &str) -> bool {
        use std::io::{self, IsTerminal, Write};

        // Check if we're in an interactive terminal
        if !io::stdin().is_terminal() {
            return false;
        }

        println!("No temporary VM found.\n");
        print!("Would you like to create one {}? [Y/n]: ", action_context);

        // If stdout flush fails, continue anyway
        let _ = io::stdout().flush();

        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(_) => {
                let input = input.trim().to_lowercase();
                // Default to 'yes' on empty input (just pressing Enter)
                input.is_empty() || input == "y" || input == "yes"
            }
            Err(_) => false,
        }
    }

    /// Simple confirmation prompt
    fn confirm_prompt(message: &str) -> bool {
        print!("{message}");
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
