//! Shared Docker command execution utilities for snapshot operations
//!
//! This module provides common Docker command execution patterns to avoid
//! code duplication across create, restore, import, and export modules.

use std::path::Path;
use std::process::Stdio;
use vm_core::error::{Result, VmError};

const ERROR_DETAIL_LIMIT: usize = 2 * 1024;

fn operation(component: &str, args: &[&str]) -> String {
    args.first().map_or_else(
        || component.to_string(),
        |subcommand| format!("{component} {subcommand}"),
    )
}

fn command_failure(operation: &str, stderr: &[u8]) -> VmError {
    let detail = String::from_utf8_lossy(stderr)
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(ERROR_DETAIL_LIMIT)
        .collect::<String>()
        .trim()
        .to_string();
    if detail.is_empty() {
        VmError::Command(format!("{operation} failed"))
    } else {
        VmError::Command(format!("{operation} failed: {detail}"))
    }
}

/// Execute docker command with streaming output (for long-running commands)
/// Output is streamed directly to the terminal so users see progress
pub async fn execute_docker_streaming(executable: &str, args: &[&str]) -> Result<()> {
    let operation = operation("docker", args);
    let status = tokio::process::Command::new(executable)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .map_err(|error| VmError::general(error, format!("Failed to execute {operation}")))?;

    if !status.success() {
        return Err(command_failure(&operation, &[]));
    }

    Ok(())
}

/// Execute docker command and return output (for commands that need captured output)
pub async fn execute_docker_with_output(executable: &str, args: &[&str]) -> Result<String> {
    let operation = operation("docker", args);
    let output = tokio::process::Command::new(executable)
        .args(args)
        .output()
        .await
        .map_err(|error| VmError::general(error, format!("Failed to execute {operation}")))?;

    if !output.status.success() {
        return Err(command_failure(&operation, &output.stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute docker command without capturing output (for quick commands like volume create/rm)
pub async fn execute_docker(executable: &str, args: &[&str]) -> Result<()> {
    let operation = operation("docker", args);
    let status = tokio::process::Command::new(executable)
        .args(args)
        .status()
        .await
        .map_err(|error| VmError::general(error, format!("Failed to execute {operation}")))?;

    if !status.success() {
        return Err(command_failure(&operation, &[]));
    }

    Ok(())
}

/// Execute docker compose command and return output
pub async fn execute_docker_compose(
    executable: &str,
    args: &[&str],
    project_dir: &Path,
) -> Result<String> {
    let operation = operation("docker compose", args);
    let output = tokio::process::Command::new(executable)
        .arg("compose")
        .args(args)
        .current_dir(project_dir)
        .output()
        .await
        .map_err(|error| VmError::general(error, format!("Failed to execute {operation}")))?;

    if !output.status.success() {
        return Err(command_failure(&operation, &output.stderr));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute docker compose command without capturing output
pub async fn execute_docker_compose_status(
    executable: &str,
    args: &[&str],
    project_dir: &Path,
) -> Result<()> {
    let operation = operation("docker compose", args);
    let status = tokio::process::Command::new(executable)
        .arg("compose")
        .args(args)
        .current_dir(project_dir)
        .status()
        .await
        .map_err(|error| VmError::general(error, format!("Failed to execute {operation}")))?;

    if !status.success() {
        return Err(command_failure(&operation, &[]));
    }

    Ok(())
}

pub async fn remove_docker_volume_if_present(executable: &str, name: &str) -> Result<()> {
    let output = tokio::process::Command::new(executable)
        .args(["volume", "rm", name])
        .output()
        .await
        .map_err(|error| VmError::general(error, "Failed to execute docker volume rm"))?;
    if output.status.success()
        || String::from_utf8_lossy(&output.stderr)
            .to_ascii_lowercase()
            .contains("no such volume")
    {
        return Ok(());
    }
    Err(command_failure("docker volume rm", &output.stderr))
}

#[cfg(test)]
mod tests {
    use super::command_failure;

    #[test]
    fn command_failures_are_bounded_and_do_not_echo_arguments() {
        let error = command_failure("docker build", "bad\n".repeat(3_000).as_bytes()).to_string();

        assert!(error.contains("docker build failed: bad"));
        assert!(error.len() < 2_100);
        assert!(!error.contains("--build-arg"));
    }
}
