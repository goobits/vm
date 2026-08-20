use tracing::info;
use vm_core::error::{Result, VmError};
use vm_core::msg;
use vm_messages::messages::MESSAGES;
use vm_provider::Provider;

use crate::{StateManager, TempVmOps};

impl TempVmOps {
    /// Show temporary VM status.
    pub fn status(provider: Box<dyn Provider>) -> Result<()> {
        let state_manager = StateManager::new().map_err(|error| {
            VmError::Internal(format!(
                "Failed to initialize state manager for status check: {error}"
            ))
        })?;
        if !state_manager.state_exists() {
            info!("{}", MESSAGES.service.temp_vm_no_vm_found);
            info!("{}", MESSAGES.service.temp_vm_create_hint);
            return Ok(());
        }

        let state = state_manager.load_state()?;
        info!("{}", MESSAGES.service.temp_vm_status);
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_container_info,
                name = &state.container_name
            )
        );
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_provider_info,
                provider = &state.provider
            )
        );
        info!(
            "   Created: {}",
            state.created_at.format("%Y-%m-%d %H:%M:%S UTC")
        );
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_project_info,
                path = state.project_dir.display().to_string()
            )
        );
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_mounts_info,
                count = state.mount_count().to_string()
            )
        );
        if state.is_auto_destroy() {
            info!("{}", MESSAGES.service.temp_vm_auto_destroy_enabled);
        }
        let report = provider.status(Some(&state.container_name))?;
        info!("   Running: {}", report.is_running);
        Ok(())
    }

    /// List all temporary VMs.
    pub fn list() -> Result<()> {
        let state_manager = StateManager::new().map_err(|error| {
            VmError::Internal(format!(
                "Failed to initialize state manager for SSH connection: {error}"
            ))
        })?;
        if !state_manager.state_exists() {
            info!("{}", MESSAGES.service.temp_vm_list_empty);
            info!("{}", MESSAGES.service.temp_vm_list_create_hint);
            return Ok(());
        }

        let state = state_manager
            .load_state()
            .map_err(|error| VmError::Internal(format!("Failed to load temp VM state: {error}")))?;
        info!("{}", MESSAGES.service.temp_vm_list_header);
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_list_item,
                name = &state.container_name,
                provider = &state.provider
            )
        );
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_list_created_date,
                date = state.created_at.format("%Y-%m-%d %H:%M:%S").to_string()
            )
        );
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_list_project,
                path = state.project_dir.display().to_string()
            )
        );
        info!(
            "{}",
            msg!(
                MESSAGES.service.temp_vm_list_mounts,
                count = state.mount_count().to_string()
            )
        );
        Ok(())
    }
}
