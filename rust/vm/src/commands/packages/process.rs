use std::io::Write;
use std::process::{Command, Output, Stdio};

use crate::error::{VmError, VmResult};

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

pub(super) fn input(command: &mut Command, content: &[u8], context: &str) -> VmResult<()> {
    let mut child = command
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| VmError::general(error, format!("Failed to {context}")))?;
    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| VmError::validation("Child stdin was unavailable", None::<String>))?
        .write_all(content);
    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(VmError::general(
            error,
            format!("Failed to stream {context}"),
        ));
    }
    let status = child.wait().map_err(|error| {
        VmError::general(error, format!("Failed to wait while trying to {context}"))
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("Could not {context} (exit status {status})"),
            Some("Run `vm packages doctor` for detailed checks"),
        ))
    }
}
