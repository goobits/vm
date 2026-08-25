use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::error::{VmError, VmResult};

use super::{write_private, ApplianceFiles};

const READ_TOKEN_FILE: &str = "read-token";
const PUBLISH_TOKEN_FILE: &str = "publish-token";
const CONTROLLER_TOKEN_FILE: &str = "controller-token";
const REVIEWER_TOKEN_FILE: &str = "reviewer-token";
const BUILD_TOKEN_FILE: &str = "build-token";
const RELEASE_TOKEN_FILE: &str = "release-token";
const ROLLOUT_TOKEN_FILE: &str = "rollout-token";
const AGENT_SIGNING_KEY_FILE: &str = "agent-signing-key";
const GIT_TOKEN_FILE: &str = "git-token";

impl ApplianceFiles {
    pub(in crate::commands::packages) fn publish_token_path(&self) -> PathBuf {
        self.root.join(PUBLISH_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn read_token_path(&self) -> PathBuf {
        self.root.join(READ_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn controller_token_path(&self) -> PathBuf {
        self.root.join(CONTROLLER_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn git_token_path(&self) -> PathBuf {
        self.root.join(GIT_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn reviewer_token_path(&self) -> PathBuf {
        self.root.join(REVIEWER_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn release_token_path(&self) -> PathBuf {
        self.root.join(RELEASE_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn build_token_path(&self) -> PathBuf {
        self.root.join(BUILD_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn rollout_token_path(&self) -> PathBuf {
        self.root.join(ROLLOUT_TOKEN_FILE)
    }

    pub(in crate::commands::packages) fn agent_signing_key_path(&self) -> PathBuf {
        self.root.join(AGENT_SIGNING_KEY_FILE)
    }

    pub(in crate::commands::packages) fn read_token(&self) -> VmResult<String> {
        self.token(&self.read_token_path())
    }

    pub(in crate::commands::packages) fn controller_token(&self) -> VmResult<String> {
        self.token(&self.controller_token_path())
    }

    pub(in crate::commands::packages) fn agent_signing_key(&self) -> VmResult<String> {
        self.token(&self.agent_signing_key_path())
    }

    pub(in crate::commands::packages) fn runtime_credentials_ready(&self) -> VmResult<bool> {
        for path in self.runtime_credential_paths() {
            match fs::read_to_string(path) {
                Ok(value) if !value.trim().is_empty() => {}
                Ok(_) => return Ok(false),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(VmError::from(error)),
            }
        }
        Ok(true)
    }

    pub(in crate::commands::packages) fn set_git_token(&self, token: &str) -> VmResult<()> {
        self.set_external_token(&self.git_token_path(), token, "Git")
    }

    pub(in crate::commands::packages) fn has_git_token(&self) -> VmResult<bool> {
        match fs::read_to_string(self.git_token_path()) {
            Ok(token) => Ok(!token.trim().is_empty()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(VmError::from(error)),
        }
    }

    pub(super) fn runtime_credential_paths(&self) -> [PathBuf; 8] {
        [
            self.read_token_path(),
            self.publish_token_path(),
            self.controller_token_path(),
            self.reviewer_token_path(),
            self.build_token_path(),
            self.release_token_path(),
            self.rollout_token_path(),
            self.agent_signing_key_path(),
        ]
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
}
