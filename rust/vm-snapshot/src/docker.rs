//! Shared Docker command execution utilities for snapshot operations
//!
//! This module provides common Docker command execution patterns to avoid
//! code duplication across create, restore, import, and export modules.

use std::path::{Path, PathBuf};
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
        .stdout(Stdio::from(std::io::stderr()))
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

#[derive(Clone)]
pub(crate) struct ComposeProject {
    pub directory: PathBuf,
    pub file: PathBuf,
}

impl ComposeProject {
    pub async fn for_environment(
        executable: &str,
        environment: Option<&str>,
        directory: &Path,
    ) -> Result<Self> {
        let environment = environment.ok_or_else(|| {
            VmError::validation("Snapshot requires a selected environment", None::<String>)
        })?;
        let files = execute_docker_with_output(
            executable,
            &[
                "inspect",
                "--format",
                "{{ index .Config.Labels \"com.docker.compose.project.config_files\" }}",
                environment,
            ],
        )
        .await?;
        if files.is_empty() || files == "<no value>" || files.contains(',') {
            return Err(VmError::validation(
                "Snapshot requires one recorded Compose configuration",
                None::<String>,
            ));
        }
        let file = PathBuf::from(files);
        if !file.is_absolute() || !file.is_file() {
            return Err(VmError::validation(
                "The environment's recorded Compose configuration is unavailable",
                None::<String>,
            ));
        }
        Ok(Self {
            directory: directory.to_path_buf(),
            file,
        })
    }
}

/// Execute docker compose command and return output
pub async fn execute_docker_compose(
    executable: &str,
    args: &[&str],
    project: &ComposeProject,
) -> Result<String> {
    let operation = operation("docker compose", args);
    let output = tokio::process::Command::new(executable)
        .arg("compose")
        .arg("--file")
        .arg(&project.file)
        .args(args)
        .current_dir(&project.directory)
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
    project: &ComposeProject,
) -> Result<()> {
    let operation = operation("docker compose", args);
    let status = tokio::process::Command::new(executable)
        .arg("compose")
        .arg("--file")
        .arg(&project.file)
        .args(args)
        .current_dir(&project.directory)
        .stdout(Stdio::from(std::io::stderr()))
        .status()
        .await
        .map_err(|error| VmError::general(error, format!("Failed to execute {operation}")))?;

    if !status.success() {
        return Err(command_failure(&operation, &[]));
    }

    Ok(())
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
