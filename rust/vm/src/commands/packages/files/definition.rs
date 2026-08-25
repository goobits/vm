use std::path::PathBuf;

use vm_packages::{ApplianceConfig, COMPOSE_YAML, GATEWAY_CONFIG};

use crate::error::{VmError, VmResult};

use super::{write_private, ApplianceFiles};

const COMPOSE_FILE: &str = "compose.yaml";
const GATEWAY_FILE: &str = "Caddyfile";
const ENVIRONMENT_FILE: &str = "environment.env";

impl ApplianceFiles {
    pub(in crate::commands::packages) fn compose_path(&self) -> PathBuf {
        self.root.join(COMPOSE_FILE)
    }

    pub(in crate::commands::packages) fn environment_path(&self) -> PathBuf {
        self.root.join(ENVIRONMENT_FILE)
    }

    pub(in crate::commands::packages) fn gateway_path(&self) -> PathBuf {
        self.root.join(GATEWAY_FILE)
    }

    pub(in crate::commands::packages) fn materialize(
        &self,
        config: &ApplianceConfig,
    ) -> VmResult<()> {
        self.ensure_root()?;
        write_private(&self.compose_path(), COMPOSE_YAML.as_bytes())?;
        write_private(&self.gateway_path(), GATEWAY_CONFIG.as_bytes())?;
        write_private(&self.environment_path(), config.environment().as_bytes())?;
        for path in self.runtime_credential_paths() {
            if !path.exists() {
                write_private(
                    &path,
                    vm_core::secrets::generate_random_password(48).as_bytes(),
                )?;
            }
        }
        if !self.git_token_path().exists() {
            write_private(&self.git_token_path(), b"")?;
        }
        Ok(())
    }

    pub(in crate::commands::packages) fn validate_definition(&self) -> VmResult<()> {
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
        assert!(files.build_token_path().is_file());
        assert!(files.release_token_path().is_file());
        assert!(files.rollout_token_path().is_file());
        assert!(files.agent_signing_key_path().is_file());
        assert!(files.git_token_path().is_file());
        assert_eq!(
            std::fs::read_to_string(files.publish_token_path())
                .unwrap()
                .len(),
            48
        );
        assert_eq!(files.read_token().unwrap().len(), 48);
        assert_eq!(files.controller_token().unwrap().len(), 48);
        assert!(!files.has_git_token().unwrap());
        assert!(files.runtime_credentials_ready().unwrap());
        files.set_git_token("github-token").unwrap();
        assert!(files.has_git_token().unwrap());
        assert!(!files.root().join("npm").exists());
    }

    #[test]
    fn materialization_repairs_missing_client_credentials() {
        let directory = tempfile::tempdir().unwrap();
        let files = ApplianceFiles::at(directory.path().join("packages"));
        let config =
            ApplianceConfig::new("127.0.0.1", 3080, "registry/image:1", "review/image:1").unwrap();

        files.materialize(&config).unwrap();
        std::fs::remove_file(files.agent_signing_key_path()).unwrap();
        assert!(!files.runtime_credentials_ready().unwrap());

        files.materialize(&config).unwrap();
        assert!(files.runtime_credentials_ready().unwrap());
        assert_eq!(files.agent_signing_key().unwrap().len(), 48);
    }
}
