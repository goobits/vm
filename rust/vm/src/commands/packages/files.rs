use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{VmError, VmResult};

use super::state::ApplianceState;

mod credentials;
mod definition;
mod locks;
mod tool_cache;

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

    pub(super) fn read_state(&self) -> VmResult<Option<ApplianceState>> {
        let path = self.root.join(STATE_FILE);
        match fs::read(&path) {
            Ok(json) => {
                let (state, migrated) = ApplianceState::from_persisted_json(&json)?;
                if migrated {
                    tracing::warn!(
                        compatibility = "legacy_package_appliance_state",
                        "package appliance state used retired fields; rewrote canonical v5 fields before v6"
                    );
                    self.write_state(&state)?;
                }
                Ok(Some(state))
            }
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
        write_private(&self.root.join(STATE_FILE), &state.to_json()?)
    }

    fn ensure_root(&self) -> VmResult<()> {
        fs::create_dir_all(&self.root).map_err(|error| {
            VmError::filesystem(
                error,
                self.root.display().to_string(),
                "create package infrastructure directory",
            )
        })?;
        vm_core::file_system::set_permissions_mode(&self.root, 0o700).map_err(VmError::from)
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
    vm_core::file_system::set_permissions_mode(path, 0o600).map_err(VmError::from)
}

#[cfg(test)]
mod tests {
    use super::ApplianceFiles;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn reading_legacy_state_rewrites_canonical_fields() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().join("packages");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("state.json"),
            r#"{
  "runtime": "docker",
  "gateway_url": "http://127.0.0.1:3080",
  "gateway_port": 3080,
  "registry_image": "registry:1",
  "review_image": "jobs:1",
  "controller_version": "5.0.1"
}"#,
        )
        .unwrap();

        let files = ApplianceFiles::at(root.clone());
        let state = files.read_state().unwrap().unwrap();
        assert_eq!(state.engine.as_str(), "docker");

        let rewritten = fs::read_to_string(root.join("state.json")).unwrap();
        assert!(rewritten.contains(r#""engine": "docker""#));
        assert!(rewritten.contains(r#""job_image": "jobs:1""#));
        assert!(!rewritten.contains("runtime"));
        assert!(!rewritten.contains("review_image"));
    }
}
