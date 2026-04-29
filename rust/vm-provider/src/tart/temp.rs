use super::{provider::tart_run_log_path, TartProvider};
use crate::{TempProvider, TempVmState, VmError};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;
use vm_core::error::Result;

#[derive(Clone, Debug)]
pub(super) struct TartDirShare {
    pub(super) tag: String,
    pub(super) host_path: PathBuf,
    pub(super) guest_path: Option<PathBuf>,
}

impl TartProvider {
    fn temp_dir_shares(state: &TempVmState) -> Vec<TartDirShare> {
        state
            .mounts
            .iter()
            .enumerate()
            .map(|(index, mount)| TartDirShare {
                tag: format!("vmtemp{index}"),
                host_path: mount.source.clone(),
                guest_path: Some(mount.target.clone()),
            })
            .collect()
    }

    fn persist_tart_dir_shares(&self, vm_name: &str, shares: &[TartDirShare]) -> Result<()> {
        for share in shares {
            let dir_arg = format!(
                "{}:{}:tag={}",
                share.tag,
                share.host_path.display(),
                share.tag
            );
            info!("Adding Tart directory share: {}", dir_arg);
            self.tart_expr(&["set", vm_name, "--dir", &dir_arg])
                .run()
                .map_err(|e| {
                    VmError::Provider(format!("Failed to add Tart directory share: {}", e))
                })?;
        }

        Ok(())
    }

    pub(super) fn mount_tart_dir_shares_in_guest(
        &self,
        vm_name: &str,
        shares: &[TartDirShare],
    ) -> Result<()> {
        let mut commands = Vec::new();
        for share in shares {
            let Some(guest_path) = share.guest_path.as_ref() else {
                continue;
            };
            let tag = Self::shell_escape_single_quotes(&share.tag);
            let target = Self::shell_escape_single_quotes(&guest_path.display().to_string());
            commands.push(format!(
                r#"is_mounted() {{
  if [ -x /sbin/mount ]; then
    /sbin/mount | grep -F "on $1 " >/dev/null 2>&1
  elif command -v mount >/dev/null 2>&1; then
    mount | grep -F "on $1 " >/dev/null 2>&1
  else
    return 1
  fi
}}
target='{target}';
if [ -x /sbin/mount_virtiofs ]; then
  mkdir -p "$target"
  if ! is_mounted "$target"; then
    /sbin/mount_virtiofs '{tag}' "$target"
  fi
else
  if ! is_mounted "$target"; then
    if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
    $SUDO mkdir -p "$target" && $SUDO mount -t virtiofs '{tag}' "$target"
  fi
fi"#
            ));
        }

        if commands.is_empty() {
            return Ok(());
        }

        self.tart_expr(&["exec", vm_name, "sh", "-c", &commands.join("\n")])
            .run()
            .map(|_| ())
            .map_err(|e| VmError::Provider(format!("Failed to mount temp directories: {}", e)))
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
