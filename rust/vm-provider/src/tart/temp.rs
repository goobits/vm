use super::{mounts::TartDirShare, provider::tart_run_log_path, TartProvider};
use crate::{TempProvider, TempVmState, VmError};
use std::time::Duration;
use tracing::info;
use vm_core::error::Result;

impl TartProvider {
    fn temp_dir_shares(state: &TempVmState) -> Vec<TartDirShare> {
        state
            .mounts
            .iter()
            .enumerate()
            .map(|(index, mount)| TartDirShare::from_mount(format!("vmtemp{index}"), mount.clone()))
            .collect()
    }
}

impl TempProvider for TartProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        info!("Updating mounts for Tart VM: {}", state.container_name);

        if self.get_instance_state(&state.container_name)?.is_none() {
            let shares = Self::temp_dir_shares(state);
            return self.create_vm_internal_with_dir_shares(
                &state.container_name,
                Some("temp"),
                &self.config,
                &shares,
            );
        }

        if self.is_instance_running(&state.container_name)? {
            self.tart_expr(&["stop", &state.container_name])
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to stop Tart temp VM: {}", e)))?;
        }

        self.recreate_with_mounts(state)?;
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        let shares = Self::temp_dir_shares(state);
        if self.get_instance_state(&state.container_name)?.is_none() {
            return self.create_vm_internal_with_dir_shares(
                &state.container_name,
                Some("temp"),
                &self.config,
                &shares,
            );
        }

        if self.is_instance_running(&state.container_name)? {
            self.tart_expr(&["stop", &state.container_name])
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to stop Tart temp VM: {}", e)))?;
        }

        self.persist_tart_dir_shares(&state.container_name, &shares)?;
        self.start_vm_background_with_dir_shares(&state.container_name, &shares)?;
        if !self.wait_for_guest_agent_ready(&state.container_name, Duration::from_secs(60)) {
            return Err(VmError::Provider(format!(
                "Tart temp VM '{}' started, but the guest agent did not become ready. Tart run log: {}",
                state.container_name,
                tart_run_log_path(&state.container_name)
            )));
        }
        self.mount_tart_dir_shares_in_guest(&state.container_name, &shares)?;
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        if !self.is_instance_running(container_name)? {
            return Ok(false);
        }

        let ssh_test = self
            .tart_expr(&["exec", container_name, "echo", "healthy"])
            .stderr_null()
            .stdout_null()
            .run();

        Ok(ssh_test.is_ok())
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        self.is_instance_running(container_name)
    }
}
