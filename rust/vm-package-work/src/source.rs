use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::warn;
use vm_packages::{sha256_hex, CheckoutRecord};

use crate::{WorkError, WorkResult};

const SOURCE_COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MAX_SOURCE_STDOUT: usize = 16 * 1024 * 1024;
const MAX_SOURCE_STDERR: usize = 64 * 1024;

mod integration;
mod rollout;
mod submission;
mod tool_build;
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
        if let Err(error) = cleanup_temporary_mirrors(mirror).await {
            warn!(
                operation = "cleanup_mirrors",
                mirror = %mirror.display(),
                error = ?error,
                "abandoned package mirror cleanup failed"
            );
        }
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

        let temporary =
            temporary_mirror_path(mirror, &vm_core::secrets::generate_random_password(12));
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
        if let Err(error) = tokio::fs::rename(&temporary, mirror).await {
            let _ = tokio::fs::remove_dir_all(&temporary).await;
            return Err(error.into());
        }
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
    let Some(prefix) = temporary_mirror_path(mirror, "")
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
    else {
        return Ok(());
    };
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

fn temporary_mirror_path(mirror: &Path, token: &str) -> PathBuf {
    let name = mirror.file_name().unwrap_or_default().to_string_lossy();
    mirror.with_file_name(format!("{name}.tmp-{token}"))
}

fn managed_component<'a>(field: &str, value: &'a str) -> WorkResult<&'a str> {
    vm_packages::validate_managed_id(field, value)?;
    Ok(value)
}

async fn run(command: &mut Command, operation: &str) -> WorkResult<Output> {
    run_with_limits(
        command,
        operation,
        SOURCE_COMMAND_TIMEOUT,
        MAX_SOURCE_STDOUT,
        MAX_SOURCE_STDERR,
    )
    .await
}

async fn run_with_limits(
    command: &mut Command,
    operation: &str,
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
) -> WorkResult<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| WorkError::Internal(format!("{operation}: {error}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| WorkError::Internal(format!("{operation}: stdout was not captured")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| WorkError::Internal(format!("{operation}: stderr was not captured")))?;
    let stdout_reader = tokio::spawn(read_bounded(stdout, stdout_limit));
    let stderr_reader = tokio::spawn(read_bounded(stderr, stderr_limit));

    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result
            .map_err(|error| WorkError::Internal(format!("{operation}: wait failed: {error}")))?,
        Err(_) => {
            terminate(&mut child).await;
            let _ = stdout_reader.await;
            let _ = stderr_reader.await;
            return Err(WorkError::Conflict(format!(
                "{operation}: command timed out after {timeout:?}"
            )));
        }
    };
    let stdout = join_reader(stdout_reader, operation).await?;
    let stderr = join_reader(stderr_reader, operation).await?;
    if stdout.exceeded {
        return Err(WorkError::Conflict(format!(
            "{operation}: stdout exceeded {stdout_limit} bytes"
        )));
    }
    if stderr.exceeded {
        return Err(WorkError::Conflict(format!(
            "{operation}: stderr exceeded {stderr_limit} bytes"
        )));
    }
    let output = Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    };
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = sanitized_diagnostic(&output.stderr);
        Err(WorkError::Conflict(format!("{operation}: {stderr}")))
    }
}

struct BoundedOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

async fn read_bounded(
    mut reader: impl AsyncRead + Unpin,
    limit: usize,
) -> io::Result<BoundedOutput> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut exceeded = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        exceeded |= read > remaining;
    }
    Ok(BoundedOutput { bytes, exceeded })
}

async fn join_reader(
    reader: tokio::task::JoinHandle<io::Result<BoundedOutput>>,
    operation: &str,
) -> WorkResult<BoundedOutput> {
    reader
        .await
        .map_err(|error| WorkError::Internal(format!("{operation}: output task failed: {error}")))?
        .map_err(|error| WorkError::Internal(format!("{operation}: output read failed: {error}")))
}

fn sanitized_diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

async fn terminate(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;

        if let Some(id) = child.id() {
            let _ = killpg(Pid::from_raw(id as i32), Signal::SIGKILL);
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
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
