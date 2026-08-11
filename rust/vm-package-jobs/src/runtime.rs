use std::fs;
use std::process::{Command, Output};

use anyhow::{bail, Context, Result};
use vm_packages::sha256_hex;

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
