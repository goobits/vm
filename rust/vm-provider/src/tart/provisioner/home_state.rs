use super::TartProvisioner;
use vm_core::error::Result;

impl TartProvisioner {
    /// Apply the provider-neutral home repair.
    pub(crate) fn repair_home_state(&self) -> Result<()> {
        self.ssh_exec(&self.home_state_repair_command())?;
        Ok(())
    }

    pub(super) fn home_state_repair_command(&self) -> String {
        format!(
            r#"set -euo pipefail
repair_script="$(mktemp)"
trap 'rm -f "$repair_script"' EXIT
cat > "$repair_script" <<'VM_HOME_STATE_REPAIR'
{}
VM_HOME_STATE_REPAIR
if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  sudo -n bash "$repair_script" "$HOME" "$(id -un)"
else
  bash "$repair_script" "$HOME" "$(id -un)"
fi"#,
            crate::resources::HOME_STATE_REPAIR
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::resources::HOME_STATE_REPAIR;

    #[test]
    fn shared_repair_skips_mounted_tool_state() {
        assert!(HOME_STATE_REPAIR.contains("is_mountpoint \"$path\" && continue"));
        assert!(HOME_STATE_REPAIR.contains("find \"$path\" -xdev"));
    }
}
