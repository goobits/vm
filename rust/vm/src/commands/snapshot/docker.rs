//! Shared Docker command execution utilities for snapshot operations
//!
//! This module provides common Docker command execution patterns to avoid
//! code duplication across create, restore, import, and export modules.

use crate::error::{VmError, VmResult};
use std::path::Path;
use std::process::Stdio;

/// Execute docker command with streaming output (for long-running commands)
/// Output is streamed directly to the terminal so users see progress
pub async fn execute_docker_streaming(args: &[&str]) -> VmResult<()> {
    let status = tokio::process::Command::new("docker")
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker command"))?;

    if !status.success() {
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker command failed"),
            format!("Command 'docker {}' failed", args.join(" ")),
        ));
    }

    Ok(())
}

/// Execute docker command and return output (for commands that need captured output)
pub async fn execute_docker_with_output(args: &[&str]) -> VmResult<String> {
    let output = tokio::process::Command::new("docker")
        .args(args)
        .output()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker command"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker command failed"),
            format!("Command failed: {}", stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute docker command without capturing output (for quick commands like volume create/rm)
pub async fn execute_docker(args: &[&str]) -> VmResult<()> {
    let status = tokio::process::Command::new("docker")
        .args(args)
        .status()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker command"))?;

    if !status.success() {
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker command failed"),
            format!("Command 'docker {}' failed", args.join(" ")),
        ));
    }

    Ok(())
}

/// Execute docker compose command and return output
pub async fn execute_docker_compose(args: &[&str], project_dir: &Path) -> VmResult<String> {
    let output = tokio::process::Command::new("docker")
        .arg("compose")
        .args(args)
        .current_dir(project_dir)
        .output()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker compose command"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker compose command failed"),
            format!("Command failed: {}", stderr),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Execute docker compose command without capturing output
pub async fn execute_docker_compose_status(args: &[&str], project_dir: &Path) -> VmResult<()> {
    let status = tokio::process::Command::new("docker")
        .arg("compose")
        .args(args)
        .current_dir(project_dir)
        .status()
        .await
        .map_err(|e| VmError::general(e, "Failed to execute docker compose command"))?;

    if !status.success() {
        return Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker compose command failed"),
            format!("Command 'docker compose {}' failed", args.join(" ")),
        ));
    }

    Ok(())
}
