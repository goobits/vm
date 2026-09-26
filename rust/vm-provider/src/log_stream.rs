//! Byte-preserving transport for structured log output.

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc;

use vm_core::error::{Result, VmError};

/// One application log record. `bytes` includes the original line ending.
#[derive(Debug, PartialEq, Eq)]
pub struct LogRecord {
    pub stream: &'static str,
    pub timestamp: Option<String>,
    pub bytes: Vec<u8>,
}

fn decode_record(stream: &'static str, bytes: Vec<u8>, timestamps: bool) -> LogRecord {
    if timestamps {
        if let Some(separator) = bytes.iter().position(|byte| *byte == b' ') {
            if let Ok(prefix) = std::str::from_utf8(&bytes[..separator]) {
                if chrono::DateTime::parse_from_rfc3339(prefix).is_ok() {
                    return LogRecord {
                        stream,
                        timestamp: Some(prefix.to_string()),
                        bytes: bytes[separator + 1..].to_vec(),
                    };
                }
            }
        }
    }
    LogRecord {
        stream,
        timestamp: None,
        bytes,
    }
}

fn read_records(
    reader: impl Read,
    stream: &'static str,
    timestamps: bool,
    sender: mpsc::Sender<std::io::Result<LogRecord>>,
) {
    let mut reader = BufReader::new(reader);
    loop {
        let mut bytes = Vec::new();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) => break,
            Ok(_)
                if sender
                    .send(Ok(decode_record(stream, bytes, timestamps)))
                    .is_err() =>
            {
                break
            }
            Ok(_) => {}
            Err(error) => {
                let _ = sender.send(Err(error));
                break;
            }
        }
    }
}

/// Read both process streams concurrently so neither can block the other.
pub(crate) fn stream_log_command(
    command: &mut Command,
    timestamps: bool,
    sink: &mut dyn FnMut(LogRecord) -> Result<()>,
) -> Result<()> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| VmError::Provider(format!("Failed to start log stream: {error}")))?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let (sender, receiver) = mpsc::channel::<std::io::Result<LogRecord>>();
    std::thread::scope(|scope| {
        for (stream, reader) in [
            ("stdout", Box::new(stdout) as Box<dyn std::io::Read + Send>),
            ("stderr", Box::new(stderr) as Box<dyn std::io::Read + Send>),
        ] {
            let sender = sender.clone();
            scope.spawn(move || read_records(reader, stream, timestamps, sender));
        }
        drop(sender);
        for record in receiver {
            let result = record
                .map_err(|error| VmError::Provider(format!("Failed to read log stream: {error}")))
                .and_then(&mut *sink);
            if let Err(error) = result {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }
        let status = child.wait().map_err(|error| {
            VmError::Provider(format!("Failed to wait for log stream: {error}"))
        })?;
        if status.success() {
            Ok(())
        } else {
            Err(VmError::Provider(format!(
                "Log stream exited with {status}"
            )))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::decode_record;
    #[cfg(unix)]
    use super::stream_log_command;

    #[test]
    fn timestamp_is_separate_from_unmodified_application_bytes() {
        let record = decode_record(
            "stdout",
            b"2026-09-25T12:34:56.123Z {\"not_json\":\xff}\n".to_vec(),
            true,
        );
        assert_eq!(
            record.timestamp.as_deref(),
            Some("2026-09-25T12:34:56.123Z")
        );
        assert_eq!(record.bytes, b"{\"not_json\":\xff}\n");
        let record = decode_record("stderr", b"raw \xff\n".to_vec(), true);
        assert_eq!(record.timestamp, None);
        assert_eq!(record.bytes, b"raw \xff\n");
    }

    #[cfg(unix)]
    #[test]
    fn streaming_preserves_binary_bytes_and_reports_failure() {
        let mut records = Vec::new();
        let error = stream_log_command(
            std::process::Command::new("sh").args([
                "-c",
                "printf '\\377out\\n'; printf '\\376err\\n' >&2; exit 7",
            ]),
            false,
            &mut |record| {
                records.push(record);
                Ok(())
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("exit status: 7"));
        assert!(records
            .iter()
            .any(|record| record.stream == "stdout" && record.bytes == b"\xffout\n"));
        assert!(records
            .iter()
            .any(|record| record.stream == "stderr" && record.bytes == b"\xfeerr\n"));
    }
}
