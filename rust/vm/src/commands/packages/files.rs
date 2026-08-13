use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use crate::error::{VmError, VmResult};
use vm_packages::{ApplianceConfig, ApplianceState, COMPOSE_YAML, GATEWAY_CONFIG};

const COMPOSE_FILE: &str = "compose.yaml";
const GATEWAY_FILE: &str = "Caddyfile";
const ENVIRONMENT_FILE: &str = "environment.env";
const READ_TOKEN_FILE: &str = "read-token";
const PUBLISH_TOKEN_FILE: &str = "publish-token";
const CONTROLLER_TOKEN_FILE: &str = "controller-token";
const REVIEWER_TOKEN_FILE: &str = "reviewer-token";
const RELEASE_TOKEN_FILE: &str = "release-token";
const ROLLOUT_TOKEN_FILE: &str = "rollout-token";
const AGENT_SIGNING_KEY_FILE: &str = "agent-signing-key";
const GIT_TOKEN_FILE: &str = "git-token";
const CI_PUBLISH_TOKEN_FILE: &str = "ci-publish-token";
const STATE_FILE: &str = "state.json";
const MAINTENANCE_LOCK_FILE: &str = "maintenance.lock";
const TOOL_CACHE_LOCK_FILE: &str = "tool-cache.lock";

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

    pub(super) fn gateway_path(&self) -> PathBuf {
        self.root.join(GATEWAY_FILE)
    }

    pub(super) fn publish_token_path(&self) -> PathBuf {
        self.root.join(PUBLISH_TOKEN_FILE)
    }

    pub(super) fn read_token_path(&self) -> PathBuf {
        self.root.join(READ_TOKEN_FILE)
    }

    pub(super) fn controller_token_path(&self) -> PathBuf {
        self.root.join(CONTROLLER_TOKEN_FILE)
    }

    pub(super) fn git_token_path(&self) -> PathBuf {
        self.root.join(GIT_TOKEN_FILE)
    }

    pub(super) fn ci_publish_token_path(&self) -> PathBuf {
        self.root.join(CI_PUBLISH_TOKEN_FILE)
    }

    pub(super) fn reviewer_token_path(&self) -> PathBuf {
        self.root.join(REVIEWER_TOKEN_FILE)
    }

    pub(super) fn release_token_path(&self) -> PathBuf {
        self.root.join(RELEASE_TOKEN_FILE)
    }

    pub(super) fn rollout_token_path(&self) -> PathBuf {
        self.root.join(ROLLOUT_TOKEN_FILE)
    }

    pub(super) fn agent_signing_key_path(&self) -> PathBuf {
        self.root.join(AGENT_SIGNING_KEY_FILE)
    }

    pub(super) fn read_token(&self) -> VmResult<String> {
        self.token(&self.read_token_path())
    }

    pub(super) fn controller_token(&self) -> VmResult<String> {
        self.token(&self.controller_token_path())
    }

    pub(super) fn agent_signing_key(&self) -> VmResult<String> {
        self.token(&self.agent_signing_key_path())
    }

    pub(super) fn set_git_token(&self, token: &str) -> VmResult<()> {
        self.set_external_token(&self.git_token_path(), token, "Git")
    }

    pub(super) fn has_git_token(&self) -> VmResult<bool> {
        match fs::read_to_string(self.git_token_path()) {
            Ok(token) => Ok(!token.trim().is_empty()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(VmError::from(error)),
        }
    }

    pub(super) fn set_ci_publish_token(&self, token: &str) -> VmResult<()> {
        self.set_external_token(&self.ci_publish_token_path(), token, "CI registry")
    }

    fn set_external_token(&self, path: &Path, token: &str, kind: &str) -> VmResult<()> {
        if token.contains(['\r', '\n']) {
            return Err(VmError::validation(
                format!("{kind} token must be a single line"),
                Some("Pass a file containing only the token"),
            ));
        }
        self.ensure_root()?;
        write_private(path, token.as_bytes())
    }

    fn token(&self, path: &Path) -> VmResult<String> {
        let token = fs::read_to_string(path).map_err(|error| {
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

    pub(super) fn acquire_maintenance_lock(&self) -> VmResult<File> {
        use fs2::FileExt;

        let file = self.lock_file()?;
        file.try_lock_exclusive().map_err(|_| {
            VmError::validation(
                "Another package backup or restore is already running",
                Some("Wait for it to finish, then retry"),
            )
        })?;
        Ok(file)
    }

    pub(super) fn acquire_operation_lock(&self) -> VmResult<File> {
        let file = self.lock_file()?;
        fs2::FileExt::try_lock_shared(&file).map_err(|_| {
            VmError::validation(
                "Package infrastructure maintenance is running",
                Some("Wait for it to finish, then retry"),
            )
        })?;
        Ok(file)
    }

    pub(super) fn acquire_lifecycle_lock(&self) -> VmResult<File> {
        let file = self.lock_file()?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_| {
            VmError::validation(
                "Another package infrastructure operation is running",
                Some("Wait for it to finish, then retry"),
            )
        })?;
        Ok(file)
    }

    pub(super) fn acquire_tool_cache_lock(&self) -> VmResult<Option<File>> {
        use fs2::FileExt;

        self.ensure_root()?;
        let path = self.root.join(TOOL_CACHE_LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| {
                VmError::filesystem(
                    error,
                    path.display().to_string(),
                    "open tool catalog cache lock",
                )
            })?;
        set_mode(&path, 0o600)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(VmError::from(error)),
        }
    }

    pub(super) fn read_tool_cache(
        &self,
        name: &str,
        max_age: std::time::Duration,
    ) -> VmResult<Option<Vec<u8>>> {
        let path = self.root.join("tool-cache").join(name);
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(VmError::from(error)),
        };
        let fresh = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age <= max_age);
        if !fresh {
            return Ok(None);
        }
        fs::read(&path).map(Some).map_err(VmError::from)
    }

    pub(super) fn write_tool_cache(&self, name: &str, content: &[u8]) -> VmResult<()> {
        let path = self.root.join("tool-cache").join(name);
        let parent = path.parent().expect("tool cache path has a parent");
        fs::create_dir_all(parent).map_err(VmError::from)?;
        set_mode(parent, 0o700)?;
        write_private(&path, content)
    }

    fn lock_file(&self) -> VmResult<File> {
        self.ensure_root()?;
        let path = self.root.join(MAINTENANCE_LOCK_FILE);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| {
                VmError::filesystem(
                    error,
                    path.display().to_string(),
                    "open package maintenance lock",
                )
            })?;
        set_mode(&path, 0o600)?;
        Ok(file)
    }

    pub(super) fn materialize(&self, config: &ApplianceConfig) -> VmResult<()> {
        self.ensure_root()?;
        write_private(&self.compose_path(), COMPOSE_YAML.as_bytes())?;
        write_private(&self.gateway_path(), GATEWAY_CONFIG.as_bytes())?;
        write_private(&self.environment_path(), config.environment().as_bytes())?;
        for path in [
            self.read_token_path(),
            self.publish_token_path(),
            self.controller_token_path(),
            self.reviewer_token_path(),
            self.release_token_path(),
            self.rollout_token_path(),
            self.agent_signing_key_path(),
        ] {
            if !path.exists() {
                let token = vm_core::secrets::generate_random_password(48);
                write_private(&path, token.as_bytes())?;
            }
        }
        if !self.git_token_path().exists() {
            write_private(&self.git_token_path(), b"")?;
        }
        if !self.ci_publish_token_path().exists() {
            write_private(&self.ci_publish_token_path(), b"")?;
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
        if COMPOSE_YAML.contains("/var/run/docker.sock")
            || COMPOSE_YAML.contains("/workspace")
            || GATEWAY_CONFIG.contains("host.docker.internal")
        {
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
        let config =
            ApplianceConfig::new("127.0.0.1", 3080, "registry/image:1", "review/image:1").unwrap();
        files.materialize(&config).unwrap();

        assert!(files.compose_path().is_file());
        assert!(files.gateway_path().is_file());
        assert!(files.environment_path().is_file());
        assert!(files.read_token_path().is_file());
        assert!(files.publish_token_path().is_file());
        assert!(files.controller_token_path().is_file());
        assert!(files.reviewer_token_path().is_file());
        assert!(files.release_token_path().is_file());
        assert!(files.rollout_token_path().is_file());
        assert!(files.agent_signing_key_path().is_file());
        assert!(files.git_token_path().is_file());
        assert!(files.ci_publish_token_path().is_file());
        assert_eq!(
            std::fs::read_to_string(files.publish_token_path())
                .unwrap()
                .len(),
            48
        );
        assert_eq!(files.read_token().unwrap().len(), 48);
        assert_eq!(files.controller_token().unwrap().len(), 48);
        assert!(!files.has_git_token().unwrap());
        files.set_git_token("github-token").unwrap();
        assert!(files.has_git_token().unwrap());
        assert!(!files.root().join("npm").exists());
    }

    #[test]
    fn maintenance_is_exclusive_while_normal_operations_can_share() {
        let directory = tempfile::tempdir().unwrap();
        let files = ApplianceFiles::at(directory.path().join("packages"));
        let first = files.acquire_operation_lock().unwrap();
        let second = files.acquire_operation_lock().unwrap();
        assert!(files.acquire_maintenance_lock().is_err());

        drop((first, second));
        let maintenance = files.acquire_maintenance_lock().unwrap();
        assert!(files.acquire_operation_lock().is_err());
        drop(maintenance);
    }
}
