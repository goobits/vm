use std::fs;
use std::future::Future;
use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Output};

use anyhow::{bail, Context, Result};
use tracing::Instrument;
use vm_packages::sha256_hex;

pub struct QueueMonitor {
    operation: &'static str,
    unavailable: bool,
}

impl QueueMonitor {
    pub fn new(operation: &'static str) -> Self {
        Self {
            operation,
            unavailable: false,
        }
    }

    pub fn available(&mut self) {
        if std::mem::take(&mut self.unavailable) {
            tracing::info!(operation = self.operation, "package queue access recovered");
        }
    }

    pub fn unavailable(&mut self, error: &impl std::fmt::Debug) {
        if !std::mem::replace(&mut self.unavailable, true) {
            tracing::warn!(
                operation = self.operation,
                error = ?error,
                "package queue unavailable"
            );
        }
    }
}

pub async fn worker_main<F>(component: &'static str, worker: F) -> ExitCode
where
    F: Future<Output = Result<()>>,
{
    let span = tracing::info_span!("package_worker", component);
    async move {
        match worker.await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(operation = "run", error = ?error, "package worker stopped");
                ExitCode::FAILURE
            }
        }
    }
    .instrument(span)
    .await
}

pub fn required_secret(variable: &str) -> Result<String> {
    let path = std::env::var(variable).with_context(|| format!("{variable} is required"))?;
    let value = fs::read_to_string(&path)
        .with_context(|| format!("failed to read secret file {path}"))?
        .trim()
        .to_string();
    if value.is_empty() {
        bail!("secret file configured by {variable} is empty");
    }
    Ok(value)
}

pub fn operation_key(operation: &str, value: &str) -> String {
    format!("{operation}-{}", &sha256_hex(value.as_bytes())[..32])
}

pub fn run_command(command: &mut Command, operation: &str) -> Result<Output> {
    let output = command
        .output()
        .with_context(|| format!("failed to {operation}"))?;
    if output.status.success() {
        return Ok(output);
    }
    bail!(
        "failed to {operation}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

pub fn command_text(command: &mut Command, operation: &str) -> Result<String> {
    Ok(String::from_utf8(run_command(command, operation)?.stdout)?)
}

pub fn authorization_header(token: &str) -> Result<tempfile::NamedTempFile> {
    let mut header = tempfile::NamedTempFile::new()?;
    writeln!(header, "Authorization: Bearer {token}")?;
    Ok(header)
}

pub fn download_bundle(url: &str, token: &str, destination: &Path) -> Result<()> {
    let header = authorization_header(token)?;
    run_command(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location"])
            .arg("--header")
            .arg(format!("@{}", header.path().display()))
            .arg("--output")
            .arg(destination)
            .arg(url),
        "download immutable source bundle",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_keys_are_stable_and_scoped() {
        let first = operation_key("release", "submission-1");
        assert_eq!(first, operation_key("release", "submission-1"));
        assert_ne!(first, operation_key("rollout", "submission-1"));
        assert_eq!(first.len(), "release-".len() + 32);
    }
}
