use super::TartProvisioner;
use crate::tart::host_sync::resolve_home_dir;
use std::path::{Path, PathBuf};
use tracing::warn;
use vm_config::config::VmConfig;
use vm_core::error::Result;

impl TartProvisioner {
    fn host_codex_auth_json_is_valid(path: &Path) -> bool {
        let Ok(file) = std::fs::File::open(path) else {
            return false;
        };

        serde_json::from_reader::<_, serde_json::Value>(file).is_ok()
    }

    fn sync_codex_auth(&self) -> Result<()> {
        let Some(home_dir) = resolve_home_dir() else {
            return Ok(());
        };
        let auth_json: PathBuf = home_dir.join(".codex/auth.json");

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

    pub(crate) fn sync_codex_runtime_config(&self, config: &VmConfig) -> Result<()> {
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

        self.sync_codex_auth()
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
