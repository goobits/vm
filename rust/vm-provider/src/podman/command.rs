//! Podman command execution utilities.
//!
//! This module provides command translation from Docker to Podman,
//! handling the differences in CLI syntax and compose tool detection.

use std::process::Command;
use vm_core::error::{Result, VmError};
use vm_core::vm_dbg;

/// Execute a Podman command, translating from Docker syntax.
///
/// This function takes Docker command arguments and translates them
/// to Podman equivalents before execution.
pub fn execute_podman_command(docker_args: &[String]) -> Result<()> {
    let podman_args = translate_docker_to_podman(docker_args);

    vm_dbg!("Executing Podman command: podman {:?}", &podman_args);

    let status = Command::new("podman")
        .args(&podman_args)
        .status()
        .map_err(|e| VmError::Internal(format!("Failed to execute podman command: {e}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(VmError::Internal(format!(
            "Podman command failed with status: {status}"
        )))
    }
}

/// Execute a Podman command and capture output.
pub fn execute_podman_with_output(docker_args: &[String]) -> Result<String> {
    let podman_args = translate_docker_to_podman(docker_args);

    vm_dbg!(
        "Executing Podman command with output: podman {:?}",
        &podman_args
    );

    let output = Command::new("podman")
        .args(&podman_args)
        .output()
        .map_err(|e| VmError::Internal(format!("Failed to execute podman command: {e}")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(VmError::Internal(format!(
            "Podman command failed: {stderr}"
        )))
    }
}

/// Translate Docker command arguments to Podman equivalents.
///
/// Most Docker commands work identically in Podman, but we may need
/// to handle special cases for compose or other features in the future.
fn translate_docker_to_podman(docker_args: &[String]) -> Vec<String> {
    // For now, most commands are identical
    // In the future, we might need to translate:
    // - docker-compose specific flags
    // - Docker-specific networking options
    // - Volume mount syntax differences
    docker_args.to_vec()
}

/// Check which compose implementation is available.
///
/// Podman supports compose in two ways:
/// 1. `podman compose` (built-in, newer versions)
/// 2. `podman-compose` (standalone tool)
///
/// Returns the compose command to use (either "compose" or path to podman-compose).
pub fn detect_compose_command() -> Result<ComposeCommand> {
    // First, try podman compose (built-in)
    let builtin_check = Command::new("podman").args(["compose", "version"]).output();

    if let Ok(output) = builtin_check {
        if output.status.success() {
            vm_dbg!("Using built-in 'podman compose'");
            return Ok(ComposeCommand::Builtin);
        }
    }

    // Fall back to podman-compose
    let standalone_check = Command::new("podman-compose").arg("version").output();

    if let Ok(output) = standalone_check {
        if output.status.success() {
            vm_dbg!("Using standalone 'podman-compose'");
            return Ok(ComposeCommand::Standalone);
        }
    }

    Err(VmError::Dependency(
        "Neither 'podman compose' nor 'podman-compose' is available. Install podman-compose: pip install podman-compose".to_string()
    ))
}

/// Compose command variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeCommand {
    /// Built-in compose (podman compose)
    Builtin,
    /// Standalone tool (podman-compose)
    Standalone,
}

impl ComposeCommand {
    /// Execute a compose command with the detected variant.
    pub fn execute(&self, compose_args: &[String]) -> Result<()> {
        match self {
            ComposeCommand::Builtin => {
                // podman compose <args>
                let mut args = vec!["compose".to_string()];
                args.extend_from_slice(compose_args);

                vm_dbg!("Executing: podman {:?}", &args);

                let status = Command::new("podman").args(&args).status().map_err(|e| {
                    VmError::Internal(format!("Failed to execute podman compose: {e}"))
                })?;

                if status.success() {
                    Ok(())
                } else {
                    Err(VmError::Internal(format!(
                        "Podman compose command failed with status: {status}"
                    )))
                }
            }
            ComposeCommand::Standalone => {
                // podman-compose <args>
                vm_dbg!("Executing: podman-compose {:?}", &compose_args);

                let status = Command::new("podman-compose")
                    .args(compose_args)
                    .status()
                    .map_err(|e| {
                        VmError::Internal(format!("Failed to execute podman-compose: {e}"))
                    })?;

                if status.success() {
                    Ok(())
                } else {
                    Err(VmError::Internal(format!(
                        "podman-compose command failed with status: {status}"
                    )))
                }
            }
        }
    }

    /// Execute a compose command and capture output.
    pub fn execute_with_output(&self, compose_args: &[String]) -> Result<String> {
        match self {
            ComposeCommand::Builtin => {
                let mut args = vec!["compose".to_string()];
                args.extend_from_slice(compose_args);

                let output = Command::new("podman").args(&args).output().map_err(|e| {
                    VmError::Internal(format!("Failed to execute podman compose: {e}"))
                })?;

                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(VmError::Internal(format!(
                        "Podman compose command failed: {stderr}"
                    )))
                }
            }
            ComposeCommand::Standalone => {
                let output = Command::new("podman-compose")
                    .args(compose_args)
                    .output()
                    .map_err(|e| {
                        VmError::Internal(format!("Failed to execute podman-compose: {e}"))
                    })?;

                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).to_string())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(VmError::Internal(format!(
                        "podman-compose command failed: {stderr}"
                    )))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_docker_to_podman_identity() {
        let docker_args = vec![
            "ps".to_string(),
            "-a".to_string(),
            "--format".to_string(),
            "{{.Names}}".to_string(),
        ];

        let podman_args = translate_docker_to_podman(&docker_args);
        assert_eq!(docker_args, podman_args);
    }

    #[test]
    fn test_compose_command_variants() {
        // Just test that the enum exists and can be created
        let builtin = ComposeCommand::Builtin;
        let standalone = ComposeCommand::Standalone;

        assert_ne!(builtin, standalone);
    }
}
