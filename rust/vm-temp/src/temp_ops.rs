// Standard library
use std::path::PathBuf;

// External crates
use tracing::{error, info};
use vm_core::error::{Result, VmError};
use vm_core::msg;
use vm_messages::messages::MESSAGES;

// Internal imports
use crate::mount_ops::parse_mount_strings;
use crate::StateManager;
use vm_config::config::VmConfig;
use vm_provider::{Provider, ProviderContext, TempVmState};

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

        let parsed_mounts = parse_mount_strings(&mounts).map_err(|e| {
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

        // Create the VM through the mount-aware temp lifecycle.
        if let Some(temp_provider) = provider.as_temp_provider() {
            temp_provider.update_mounts(&temp_state)?;
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
                MESSAGES.service.temp_vm_created_with_mounts,
                count = temp_state.mount_count().to_string()
            )
        );

        if auto_destroy {
            // SSH then destroy
            info!("{}", MESSAGES.service.temp_vm_connecting);
            provider.ssh(Some(&temp_state.container_name), &PathBuf::from("."))?;
            info!("{}", MESSAGES.service.temp_vm_auto_destroying);
            provider.destroy(
                Some(&temp_state.container_name),
                &ProviderContext::default(),
            )?;
            state_manager.delete_state()?;
        } else {
            info!("{}", MESSAGES.service.temp_vm_usage_hint);
        }

        Ok(())
    }

    /// SSH into the temporary VM
    pub fn ssh(provider: Box<dyn Provider>, _config: VmConfig) -> Result<()> {
        let state_manager = StateManager::new().map_err(|e| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {}",
                e
            ))
        })?;

        if !state_manager.state_exists() {
            return Err(VmError::NotFound(
                "No temporary VM exists; create one before connecting".to_string(),
            ));
        }

        let state = state_manager.load_state()?;
        provider.ssh(Some(&state.container_name), &PathBuf::from("."))
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

        info!("{}", MESSAGES.service.temp_vm_destroying);
        let state = state_manager.load_state()?;
        provider.destroy(Some(&state.container_name), &ProviderContext::default())?;

        state_manager.delete_state()?;

        info!("{}", MESSAGES.service.temp_vm_destroyed);
        info!("{}", MESSAGES.service.temp_vm_create_hint);
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

        info!("{}", MESSAGES.service.temp_vm_stopping);

        match provider.stop(None) {
            Ok(()) => {
                info!("{}", MESSAGES.service.temp_vm_stopped_success);
                info!("{}", MESSAGES.service.temp_vm_restart_hint);
                Ok(())
            }
            Err(e) => {
                error!("{}", MESSAGES.service.temp_vm_failed_to_stop);
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

        info!("{}", MESSAGES.service.temp_vm_starting);

        match provider.start(None, &ProviderContext::default()) {
            Ok(()) => {
                info!("{}", MESSAGES.service.temp_vm_started_success);

                // Show mount info if any
                if state.mount_count() > 0 {
                    info!(
                        "{}",
                        msg!(
                            MESSAGES.service.temp_vm_mounts_configured,
                            count = state.mount_count().to_string()
                        )
                    );
                }

                info!("{}", MESSAGES.service.temp_vm_connect_hint);
                Ok(())
            }
            Err(e) => {
                error!("{}", MESSAGES.service.temp_vm_failed_to_start);
                error!(
                    "   {}",
                    msg!(MESSAGES.common.error_generic, error = e.to_string())
                );
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

        info!("{}", MESSAGES.service.temp_vm_restarting);
        info!("{}", MESSAGES.service.temp_vm_stopping_step);
        info!("{}", MESSAGES.service.temp_vm_starting_step);

        match provider.restart(None, &ProviderContext::default()) {
            Ok(()) => {
                info!("{}", MESSAGES.service.temp_vm_services_ready);
                info!("{}", MESSAGES.service.temp_vm_restarted_success);

                if state.mount_count() > 0 {
                    info!(
                        "{}",
                        msg!(
                            MESSAGES.service.temp_vm_mounts_active,
                            count = state.mount_count().to_string()
                        )
                    );
                }

                info!("{}", MESSAGES.service.temp_vm_connect_hint);
                Ok(())
            }
            Err(e) => {
                error!("{}", MESSAGES.service.temp_vm_failed_to_restart);
                error!("   Error: {}", e);
                Err(e)
            }
        }
    }
}
