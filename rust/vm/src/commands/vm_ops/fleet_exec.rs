//! Framed output and aggregate status for multi-environment commands.

use std::io::{self, Write};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::{json, Value};
use vm_provider::{ExecOptions, GuestOutput, Provider};

use crate::cli::ExecOutput;
use crate::error::{VmError, VmResult};

pub(in crate::commands) struct ExecTarget {
    pub name: String,
    pub provider: VmResult<Box<dyn Provider>>,
}

impl ExecTarget {
    pub fn ready(name: String, provider: Box<dyn Provider>) -> Self {
        Self {
            name,
            provider: Ok(provider),
        }
    }
}

fn json_line(writer: &mut impl Write, event: &Value) -> VmResult<()> {
    serde_json::to_writer(&mut *writer, event)
        .map_err(|error| VmError::general(error, "Failed to serialize fleet exec output"))?;
    writeln!(writer)?;
    Ok(())
}

fn emit_stream(
    writer: &mut impl Write,
    output: ExecOutput,
    target: &str,
    stream: &str,
    bytes: &[u8],
) -> VmResult<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    match output {
        ExecOutput::Grouped => {
            writeln!(writer, "[{target} {stream}]")?;
            writer.write_all(bytes)?;
            if !bytes.ends_with(b"\n") {
                writeln!(writer)?;
            }
        }
        ExecOutput::JsonLines => json_line(
            writer,
            &json!({
                "schema_version": 1,
                "command": "exec",
                "type": "output",
                "target": target,
                "stream": stream,
                "encoding": "base64",
                "data_base64": STANDARD.encode(bytes),
            }),
        )?,
    }
    Ok(())
}

fn emit_capture(
    writer: &mut impl Write,
    output: ExecOutput,
    target: &str,
    capture: &GuestOutput,
) -> VmResult<()> {
    emit_stream(writer, output, target, "stdout", &capture.stdout)?;
    emit_stream(writer, output, target, "stderr", &capture.stderr)?;
    Ok(())
}

fn emit_target_result(
    writer: &mut impl Write,
    output: ExecOutput,
    target: &str,
    exit_code: Option<i32>,
    error: Option<&str>,
) -> VmResult<()> {
    match output {
        ExecOutput::Grouped => match (exit_code, error) {
            (Some(code), _) => writeln!(writer, "[{target} exit {code}]")?,
            (None, Some(error)) => writeln!(writer, "[{target} failed: {error}]")?,
            (None, None) => unreachable!("target result needs status or error"),
        },
        ExecOutput::JsonLines => json_line(
            writer,
            &json!({
                "schema_version": 1,
                "command": "exec",
                "type": "target_result",
                "target": target,
                "ok": exit_code == Some(0),
                "exit_code": exit_code,
                "error": error,
            }),
        )?,
    }
    Ok(())
}

pub(in crate::commands) fn run(
    targets: Vec<ExecTarget>,
    command: &[String],
    options: &ExecOptions,
    output: ExecOutput,
) -> VmResult<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    run_to(&mut writer, targets, command, options, output)
}

fn run_to(
    writer: &mut impl Write,
    targets: Vec<ExecTarget>,
    command: &[String],
    options: &ExecOptions,
    output: ExecOutput,
) -> VmResult<()> {
    let total = targets.len();
    let mut failed = 0;
    for target in targets {
        match output {
            ExecOutput::Grouped => writeln!(writer, "==> {}", target.name)?,
            ExecOutput::JsonLines => json_line(
                writer,
                &json!({
                    "schema_version": 1,
                    "command": "exec",
                    "type": "target_start",
                    "target": &target.name,
                    "provider": target.provider.as_ref().ok().map(|provider| provider.name()),
                }),
            )?,
        }
        writer.flush()?;
        let result = target.provider.and_then(|provider| {
            provider
                .exec_capture_with_options(Some(&target.name), command, options)
                .map_err(VmError::from)
        });
        match result {
            Ok(capture) => {
                emit_capture(writer, output, &target.name, &capture)?;
                let code = capture.status.code();
                emit_target_result(writer, output, &target.name, Some(code), None)?;
                failed += usize::from(code != 0);
            }
            Err(error) => {
                emit_target_result(writer, output, &target.name, None, Some(&error.to_string()))?;
                failed += 1;
            }
        }
    }
    let succeeded = total - failed;
    match output {
        ExecOutput::Grouped => writeln!(writer, "Fleet exec: {succeeded}/{total} succeeded")?,
        ExecOutput::JsonLines => json_line(
            writer,
            &json!({
                "schema_version": 1,
                "command": "exec",
                "type": "result",
                "ok": failed == 0,
                "targets": total,
                "succeeded": succeeded,
                "failed": failed,
            }),
        )?,
    }
    writer.flush()?;
    if failed == 0 {
        Ok(())
    } else {
        Err(VmError::general(
            io::Error::new(io::ErrorKind::Other, "fleet exec failed"),
            format!("{failed} of {total} fleet commands failed"),
        )
        .reported())
    }
}

#[cfg(test)]
mod tests {
    use super::{emit_capture, emit_target_result, run_to, ExecTarget};
    use crate::cli::ExecOutput;
    use crate::error::VmError;
    use vm_provider::{ExecOptions, GuestExit, GuestOutput};

    #[test]
    fn json_lines_preserves_non_utf8_guest_bytes() {
        let mut bytes = Vec::new();
        emit_capture(
            &mut bytes,
            ExecOutput::JsonLines,
            "demo-dev",
            &GuestOutput {
                status: GuestExit::new(42),
                stdout: vec![0xff, b'o'],
                stderr: vec![0xfe, b'e'],
            },
        )
        .unwrap();
        let events = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events[0]["stream"], "stdout");
        assert_eq!(events[0]["data_base64"], "/28=");
        assert_eq!(events[1]["stream"], "stderr");
        assert_eq!(events[1]["data_base64"], "/mU=");
    }

    #[test]
    fn target_failures_produce_a_final_failed_result() {
        let target = ExecTarget {
            name: "demo-dev".into(),
            provider: Err(VmError::validation("provider unavailable", None::<String>)),
        };
        let mut bytes = Vec::new();
        let result = run_to(
            &mut bytes,
            vec![target],
            &["true".into()],
            &ExecOptions::default(),
            ExecOutput::JsonLines,
        );
        assert_eq!(result.unwrap_err().exit_code(), 1);
        let events = bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["type"], "target_start");
        assert_eq!(events[1]["type"], "target_result");
        assert_eq!(events[1]["exit_code"], serde_json::Value::Null);
        assert_eq!(events[2]["type"], "result");
        assert_eq!(events[2]["failed"], 1);
    }

    #[test]
    fn target_result_keeps_the_exact_guest_exit_code() {
        let mut bytes = Vec::new();
        emit_target_result(
            &mut bytes,
            ExecOutput::JsonLines,
            "demo-dev",
            Some(42),
            None,
        )
        .unwrap();
        let event: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(event["exit_code"], 42);
        assert_eq!(event["ok"], false);
    }
}
