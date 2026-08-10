use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{VmError, VmResult};
use vm_packages::{ApplianceConfig, ApplianceState, COMPOSE_YAML};

const COMPOSE_FILE: &str = "compose.yaml";
const ENVIRONMENT_FILE: &str = "environment.env";
const READ_TOKEN_FILE: &str = "read-token";
const PUBLISH_TOKEN_FILE: &str = "publish-token";
const STATE_FILE: &str = "state.json";

#[derive(Debug, Clone)]
pub(super) struct ApplianceFiles {
    root: PathBuf,
}

impl ApplianceFiles {
    pub(super) fn discover() -> VmResult<Self> {
        Ok(Self {
            root: vm_core::user_paths::vm_state_dir()?
                .join("infrastructure")
                .join("packages"),
        })
    }

    #[cfg(test)]
    pub(super) fn at(root: PathBuf) -> Self {
        Self { root }
    }

    pub(super) fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn compose_path(&self) -> PathBuf {
        self.root.join(COMPOSE_FILE)
    }

    pub(super) fn environment_path(&self) -> PathBuf {
        self.root.join(ENVIRONMENT_FILE)
    }

    pub(super) fn publish_token_path(&self) -> PathBuf {
        self.root.join(PUBLISH_TOKEN_FILE)
    }

    pub(super) fn read_token_path(&self) -> PathBuf {
        self.root.join(READ_TOKEN_FILE)
    }

    pub(super) fn read_token(&self) -> VmResult<String> {
        let path = self.read_token_path();
        let token = fs::read_to_string(&path).map_err(|error| {
            VmError::filesystem(
                error,
                path.display().to_string(),
                "read package infrastructure client credential",
            )
        })?;
        Ok(token.trim().to_string())
    }

    pub(super) fn tart_log_path(&self) -> PathBuf {
        self.root.join("tart-run.log")
    }

    pub(super) fn materialize(&self, config: &ApplianceConfig) -> VmResult<()> {
        self.ensure_root()?;
        write_private(&self.compose_path(), COMPOSE_YAML.as_bytes())?;
        write_private(&self.environment_path(), config.environment().as_bytes())?;
        for path in [self.read_token_path(), self.publish_token_path()] {
            if !path.exists() {
                let token = vm_core::secrets::generate_random_password(48);
                write_private(&path, token.as_bytes())?;
            }
        }
        Ok(())
    }

    pub(super) fn read_state(&self) -> VmResult<Option<ApplianceState>> {
        let path = self.root.join(STATE_FILE);
        match fs::read(&path) {
            Ok(json) => ApplianceState::from_json(&json)
                .map(Some)
                .map_err(VmError::from),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(VmError::filesystem(
                error,
                path.display().to_string(),
                "read package infrastructure state",
            )),
        }
    }

    pub(super) fn write_state(&self, state: &ApplianceState) -> VmResult<()> {
        self.ensure_root()?;
        write_private(
            &self.root.join(STATE_FILE),
            &state.to_json().map_err(VmError::from)?,
        )
    }

    pub(super) fn validate_definition(&self) -> VmResult<()> {
        if COMPOSE_YAML.contains("/var/run/docker.sock") || COMPOSE_YAML.contains("/workspace") {
            return Err(VmError::validation(
                "Package appliance definition crosses a protected host boundary",
                Some("Registry storage and project source must remain private"),
            ));
        }
        Ok(())
    }

    fn ensure_root(&self) -> VmResult<()> {
        fs::create_dir_all(&self.root).map_err(|error| {
            VmError::filesystem(
                error,
                self.root.display().to_string(),
                "create package infrastructure directory",
            )
        })?;
        set_mode(&self.root, 0o700)
    }
}

fn write_private(path: &Path, content: &[u8]) -> VmResult<()> {
    vm_core::file_system::atomic_write(path, content).map_err(|error| {
        VmError::filesystem(
            error,
            path.display().to_string(),
            "write package infrastructure state",
        )
    })?;
    set_mode(path, 0o600)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> VmResult<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(VmError::from)
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> VmResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ApplianceFiles;
    use vm_packages::ApplianceConfig;

    #[test]
    fn materializes_controller_files_without_registry_data() {
        let directory = tempfile::tempdir().unwrap();
        let files = ApplianceFiles::at(directory.path().join("packages"));
        let config = ApplianceConfig::new("127.0.0.1", 3080, "registry/image:1").unwrap();
        files.materialize(&config).unwrap();

        assert!(files.compose_path().is_file());
        assert!(files.environment_path().is_file());
        assert!(files.read_token_path().is_file());
        assert!(files.publish_token_path().is_file());
        assert_eq!(
            std::fs::read_to_string(files.publish_token_path())
                .unwrap()
                .len(),
            48
        );
        assert_eq!(files.read_token().unwrap().len(), 48);
        assert!(!files.root().join("npm").exists());
    }
}
