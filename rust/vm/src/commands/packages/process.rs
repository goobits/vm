use std::process::{Command, Output};

use crate::error::{VmError, VmResult};

pub(super) fn validate_job_id(value: &str) -> VmResult<()> {
    vm_packages::validate_managed_id("package job identifier", value).map_err(VmError::from)
}

pub(super) fn run(command: &mut Command, context: &str) -> VmResult<()> {
    let status = command
        .status()
        .map_err(|error| VmError::general(error, format!("Failed to {context}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("Could not {context} (exit status {status})"),
            Some("Run `vm packages doctor` for detailed checks"),
        ))
    }
}

pub(super) fn output(command: &mut Command, context: &str) -> VmResult<Output> {
    let output = command
        .output()
        .map_err(|error| VmError::general(error, format!("Failed to {context}")))?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(VmError::validation(
            format!("Could not {context}: {stderr}"),
            Some("Run `vm packages doctor` for detailed checks"),
        ))
    }
}
