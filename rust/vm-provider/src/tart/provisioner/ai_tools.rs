use super::{resolve_home_dir, Path, PathBuf, Result, TartProvisioner, VmConfig};
use tracing::warn;

impl TartProvisioner {
    fn prepare_codex_home(&self) -> Result<()> {
        self.ssh_exec(
            r#"set -e
codex_home="$HOME/.codex"
home_uid="$(stat -f %u "$HOME" 2>/dev/null || stat -c %u "$HOME" 2>/dev/null || id -u)"
home_gid="$(stat -f %g "$HOME" 2>/dev/null || stat -c %g "$HOME" 2>/dev/null || id -g)"
if command -v sudo >/dev/null 2>&1; then SUDO=sudo; else SUDO=""; fi
$SUDO chflags -R nouchg,noschg "$codex_home" "$HOME/.zshrc" "$HOME/.bashrc" 2>/dev/null || true
is_mounted() {
  if [ -x /sbin/mount ]; then
    /sbin/mount | grep -F "on $1 " >/dev/null 2>&1
  elif command -v mount >/dev/null 2>&1; then
    mount | grep -F "on $1 " >/dev/null 2>&1
  else
    return 1
  fi
}
if is_mounted "$codex_home"; then
  $SUDO umount "$codex_home"
fi
mkdir -p "$codex_home/bin" "$codex_home/log" "$codex_home/sessions" "$codex_home/rollout"
touch "$codex_home/config.toml"
$SUDO chown -R "$home_uid:$home_gid" "$codex_home" "$HOME/.zshrc" "$HOME/.bashrc" 2>/dev/null || true
chmod u+rw "$HOME/.zshrc" "$HOME/.bashrc" 2>/dev/null || true
chmod 700 "$codex_home" "$codex_home/bin" "$codex_home/log" "$codex_home/sessions" "$codex_home/rollout"
chmod 600 "$codex_home/config.toml" 2>/dev/null || true
repair_auth_json() {
  auth_json="$codex_home/auth.json"
  if [ ! -f "$auth_json" ]; then
    return 0
  fi
  if [ ! -s "$auth_json" ]; then
    rm -f "$auth_json"
    return 0
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 - "$auth_json" <<'PY' || rm -f "$auth_json"
import json
import sys
with open(sys.argv[1], encoding="utf-8") as f:
    json.load(f)
PY
  elif command -v plutil >/dev/null 2>&1; then
    plutil -lint "$auth_json" >/dev/null 2>&1 || rm -f "$auth_json"
  elif command -v node >/dev/null 2>&1; then
    node -e 'JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"))' "$auth_json" || rm -f "$auth_json"
  fi
  if [ -f "$auth_json" ]; then chmod 600 "$auth_json"; fi
}
repair_auth_json"#,
        )?;

        Ok(())
    }

    fn host_codex_auth_json_is_valid(path: &Path) -> bool {
        let Ok(file) = std::fs::File::open(path) else {
            return false;
        };

        serde_json::from_reader::<_, serde_json::Value>(file).is_ok()
    }

    pub(crate) fn ensure_codex_runtime_config(&self, config: &VmConfig) -> Result<()> {
        let Some(ai_tools) = config
            .host_sync
            .as_ref()
            .and_then(|sync| sync.ai_tools.as_ref())
        else {
            return Ok(());
        };
        if !ai_tools.is_codex_enabled() {
            return Ok(());
        }

        self.prepare_codex_home()?;

        let Some(home_dir) = resolve_home_dir() else {
            return Ok(());
        };
        let codex_dir: PathBuf = home_dir.join(".codex");

        let auth_json = codex_dir.join("auth.json");
        if auth_json
            .metadata()
            .is_ok_and(|metadata| metadata.len() > 0)
            && Self::host_codex_auth_json_is_valid(&auth_json)
        {
            self.copy_host_file_to_guest_home(&auth_json, ".codex/auth.json", "600")?;
        } else if auth_json.exists() {
            warn!(
                "Skipping invalid or empty Codex auth file while provisioning Tart: {}",
                auth_json.display()
            );
        }

        Ok(())
    }

    pub(super) fn provision_ai_tools(&self, config: &VmConfig) -> Result<()> {
        let Some(ai_tools) = config
            .host_sync
            .as_ref()
            .and_then(|sync| sync.ai_tools.as_ref())
        else {
            return Ok(());
        };

        if ai_tools.is_claude_enabled() {
            self.ssh_exec(&format!(
                r#"export PATH="{}"
if ! command -v claude >/dev/null 2>&1; then
  curl -fsSL https://claude.ai/install.sh | bash
fi"#,
                Self::user_bin_path(config)
            ))?;
        }

        if ai_tools.is_gemini_enabled() || ai_tools.is_codex_enabled() {
            self.ensure_nodejs_runtime(config)?;
        }

        if ai_tools.is_gemini_enabled() {
            self.ssh_exec(&format!(
                r#"export PATH="{}"
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
if ! command -v gemini >/dev/null 2>&1; then
  npm install -g @google/gemini-cli
fi"#,
                Self::user_bin_path(config)
            ))?;
        }

        if ai_tools.is_codex_enabled() {
            self.ssh_exec(&format!(
                r#"export PATH="{}"
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
if ! command -v codex >/dev/null 2>&1; then
  npm install -g @openai/codex
fi"#,
                Self::user_bin_path(config)
            ))?;
            self.ensure_codex_runtime_config(config)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TartProvisioner;
    use std::fs;

    #[test]
    fn host_codex_auth_validation_accepts_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let auth_json = temp_dir.path().join("auth.json");
        fs::write(&auth_json, r#"{"OPENAI_API_KEY":"test"}"#).unwrap();

        assert!(TartProvisioner::host_codex_auth_json_is_valid(&auth_json));
    }

    #[test]
    fn host_codex_auth_validation_rejects_empty_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let auth_json = temp_dir.path().join("auth.json");
        fs::write(&auth_json, "").unwrap();

        assert!(!TartProvisioner::host_codex_auth_json_is_valid(&auth_json));
    }

    #[test]
    fn host_codex_auth_validation_rejects_invalid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let auth_json = temp_dir.path().join("auth.json");
        fs::write(&auth_json, "not json").unwrap();

        assert!(!TartProvisioner::host_codex_auth_json_is_valid(&auth_json));
    }
}
