use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::Arc;

use tokio::process::Command;
use tokio::sync::Mutex;
use vm_packages::{sha256_hex, CheckoutRecord};

use crate::{WorkError, WorkResult};

mod integration;
mod rollout;
mod submission;
mod worktree;

#[derive(Clone)]
pub(crate) struct SourceManager {
    root: PathBuf,
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl SourceManager {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn sync_mirror(&self, mirror: &Path, repository: &str) -> WorkResult<()> {
        if mirror.is_dir() {
            return run(
                self.git()
                    .arg("--git-dir")
                    .arg(mirror)
                    .arg("remote")
                    .arg("update")
                    .arg("--prune"),
                "update canonical package mirror",
            )
            .await
            .map(|_| ());
        }

        cleanup_temporary_mirrors(mirror).await?;

        let temporary = mirror.with_extension(format!(
            "tmp-{}",
            vm_core::secrets::generate_random_password(12)
        ));
        let clone = run(
            self.git()
                .arg("clone")
                .arg("--mirror")
                .arg(repository)
                .arg(&temporary),
            "clone canonical package mirror",
        )
        .await;
        if let Err(error) = clone {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err(error);
        }
        tokio::fs::rename(temporary, mirror).await?;
        Ok(())
    }

    fn git(&self) -> Command {
        let mut command = Command::new("git");
        command.kill_on_drop(true);
        command.env("GIT_TERMINAL_PROMPT", "0");
        if let Ok(askpass) = std::env::var("PKG_WORK_GIT_ASKPASS") {
            command.env("GIT_ASKPASS", askpass);
        }
        if let Ok(token_file) = std::env::var("PKG_WORK_GIT_TOKEN_FILE") {
            for config in vm_packages::AUTHENTICATED_GIT_CONFIG {
                command.args(["-c", config]);
            }
            command.env("PKG_WORK_GIT_TOKEN_FILE", token_file);
        }
        command
    }

    async fn lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks
            .entry(key.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    fn checkout_source(&self, checkout: &CheckoutRecord) -> WorkResult<PathBuf> {
        let source = checkout
            .worktree
            .as_deref()
            .map(PathBuf::from)
            .ok_or_else(|| WorkError::Conflict("checkout source is not ready".into()))?;
        let expected = self.agent_root(&checkout.checkout_id)?.join("source");
        if source != expected {
            return Err(WorkError::Internal(
                "checkout source escaped its managed directory".into(),
            ));
        }
        Ok(source)
    }

    fn agent_root(&self, checkout_id: &str) -> WorkResult<PathBuf> {
        Ok(self
            .root
            .join("agents")
            .join(managed_component("checkout ID", checkout_id)?))
    }
}

async fn cleanup_temporary_mirrors(mirror: &Path) -> WorkResult<()> {
    let Some(parent) = mirror.parent() else {
        return Ok(());
    };
    let Some(name) = mirror.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let prefix = format!("{name}.tmp-");
    let mut entries = match tokio::fs::read_dir(parent).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|candidate| candidate.starts_with(&prefix))
        {
            let path = entry.path();
            if entry.file_type().await?.is_dir() {
                tokio::fs::remove_dir_all(path).await?;
            } else {
                tokio::fs::remove_file(path).await?;
            }
        }
    }
    Ok(())
}

fn managed_component<'a>(field: &str, value: &'a str) -> WorkResult<&'a str> {
    vm_packages::validate_managed_id(field, value)?;
    Ok(value)
}

async fn run(command: &mut Command, operation: &str) -> WorkResult<Output> {
    let output = command
        .output()
        .await
        .map_err(|error| WorkError::Internal(format!("{operation}: {error}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(WorkError::Conflict(format!("{operation}: {stderr}")))
    }
}

async fn git_output(command: &mut Command, operation: &str) -> WorkResult<String> {
    let output = run(command, operation).await?;
    let value = String::from_utf8(output.stdout)
        .map_err(|_| WorkError::Internal(format!("{operation}: invalid UTF-8")))?;
    Ok(value.trim().to_string())
}

fn source_key(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let suffix = sha256_hex(value.as_bytes());
    let suffix = &suffix[..8];
    format!("{}-{suffix}", slug.trim_matches('-'))
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
