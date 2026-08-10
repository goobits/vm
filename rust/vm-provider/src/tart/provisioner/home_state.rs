use super::TartProvisioner;
use vm_core::error::Result;

impl TartProvisioner {
    /// Migrate obsolete mounts once, then apply the provider-neutral home repair.
    pub(crate) fn repair_home_state(&self) -> Result<()> {
        self.ssh_exec(&self.home_state_repair_command())?;
        Ok(())
    }

    pub(super) fn home_state_repair_command(&self) -> String {
        format!(
            r#"set -euo pipefail
legacy_marker="$HOME/.vm/state/tart-codex-mount-v1"
legacy_migrated=0
if [ ! -f "$legacy_marker" ]; then
  is_mounted() {{
    if [ -x /sbin/mount ]; then
      /sbin/mount | grep -F "on $1 " >/dev/null 2>&1
    elif command -v mount >/dev/null 2>&1; then
      mount | grep -F "on $1 " >/dev/null 2>&1
    else
      return 1
    fi
  }}
  if is_mounted "$HOME/.codex"; then
    if command -v sudo >/dev/null 2>&1; then
      sudo -n umount "$HOME/.codex"
    else
      umount "$HOME/.codex"
    fi
  fi
  legacy_migrated=1
fi

repair_script="$(mktemp)"
trap 'rm -f "$repair_script"' EXIT
cat > "$repair_script" <<'VM_HOME_STATE_REPAIR'
{}
VM_HOME_STATE_REPAIR
if command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
  sudo -n bash "$repair_script" "$HOME" "$(id -un)"
else
  bash "$repair_script" "$HOME" "$(id -un)"
fi
if [ "$legacy_migrated" = 1 ]; then
  touch "$legacy_marker"
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
