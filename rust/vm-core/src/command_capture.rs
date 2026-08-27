use std::io::{self, Read};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

#[derive(Clone, Copy, Debug)]
pub struct CaptureLimits {
    pub timeout: Duration,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
}

impl CaptureLimits {
    pub const fn new(timeout: Duration, stdout_bytes: usize, stderr_bytes: usize) -> Self {
        Self {
            timeout,
            stdout_bytes,
            stderr_bytes,
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandCaptureError {
    #[error("failed to {operation}: {source}")]
    Spawn {
        operation: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to {operation}: wait failed: {source}")]
    Wait {
        operation: String,
        #[source]
        source: io::Error,
    },
    #[error("failed to {operation}: {stream} read failed: {source}")]
    Read {
        operation: String,
        stream: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("failed to {operation}: {stream} reader task failed: {message}")]
    ReaderTask {
        operation: String,
        stream: &'static str,
        message: String,
    },
    #[error("failed to {operation}: {stream} reader panicked")]
    ReaderPanicked {
        operation: String,
        stream: &'static str,
    },
    #[error("failed to {operation}: command timed out after {timeout:?}")]
    Timeout {
        operation: String,
        timeout: Duration,
    },
    #[error("failed to {operation}: command {stream} exceeded {limit} bytes")]
    OutputLimit {
        operation: String,
        stream: &'static str,
        limit: usize,
    },
}

impl CommandCaptureError {
    pub fn is_command_constraint(&self) -> bool {
        matches!(self, Self::Timeout { .. } | Self::OutputLimit { .. })
    }

    /// Whether the command runner itself failed rather than the child command.
    pub fn is_infrastructure_failure(&self) -> bool {
        matches!(
            self,
            Self::Spawn { .. }
                | Self::Wait { .. }
                | Self::Read { .. }
                | Self::ReaderTask { .. }
                | Self::ReaderPanicked { .. }
        )
    }
}

pub fn capture_output(
    command: &mut Command,
    operation: &str,
    limits: CaptureLimits,
) -> Result<Output, CommandCaptureError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    configure_process_group(command);
    let mut child = command
        .spawn()
        .map_err(|source| CommandCaptureError::Spawn {
            operation: operation.to_string(),
            source,
        })?;
    let Some(stdout) = child.stdout.take() else {
        terminate_sync(&mut child);
        return Err(CommandCaptureError::ReaderTask {
            operation: operation.to_string(),
            stream: "stdout",
            message: "stream was not captured".into(),
        });
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_sync(&mut child);
        return Err(CommandCaptureError::ReaderTask {
            operation: operation.to_string(),
            stream: "stderr",
            message: "stream was not captured".into(),
        });
    };
    let stdout_reader = thread::spawn(move || read_bounded(stdout, limits.stdout_bytes));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, limits.stderr_bytes));
    let deadline = Instant::now() + limits.timeout;

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(source) => {
                terminate_sync(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(CommandCaptureError::Wait {
                    operation: operation.to_string(),
                    source,
                });
            }
        }
        if Instant::now() >= deadline {
            terminate_sync(&mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(CommandCaptureError::Timeout {
                operation: operation.to_string(),
                timeout: limits.timeout,
            });
        }
        thread::sleep(Duration::from_millis(50));
    };

    let stdout = join_sync_reader(stdout_reader, operation, "stdout")?;
    let stderr = join_sync_reader(stderr_reader, operation, "stderr")?;
    output(status, stdout, stderr, operation, limits)
}

pub async fn capture_output_async(
    command: &mut tokio::process::Command,
    operation: &str,
    limits: CaptureLimits,
) -> Result<Output, CommandCaptureError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|source| CommandCaptureError::Spawn {
            operation: operation.to_string(),
            source,
        })?;
    let Some(stdout) = child.stdout.take() else {
        terminate_async(&mut child).await;
        return Err(CommandCaptureError::ReaderTask {
            operation: operation.to_string(),
            stream: "stdout",
            message: "stream was not captured".into(),
        });
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_async(&mut child).await;
        return Err(CommandCaptureError::ReaderTask {
            operation: operation.to_string(),
            stream: "stderr",
            message: "stream was not captured".into(),
        });
    };
    let stdout_reader = tokio::spawn(read_bounded_async(stdout, limits.stdout_bytes));
    let stderr_reader = tokio::spawn(read_bounded_async(stderr, limits.stderr_bytes));

    let status = match tokio::time::timeout(limits.timeout, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(source)) => {
            terminate_async(&mut child).await;
            stdout_reader.abort();
            stderr_reader.abort();
            return Err(CommandCaptureError::Wait {
                operation: operation.to_string(),
                source,
            });
        }
        Err(_) => {
            terminate_async(&mut child).await;
            let _ = stdout_reader.await;
            let _ = stderr_reader.await;
            return Err(CommandCaptureError::Timeout {
                operation: operation.to_string(),
                timeout: limits.timeout,
            });
        }
    };
    let stdout = join_async_reader(stdout_reader, operation, "stdout").await?;
    let stderr = join_async_reader(stderr_reader, operation, "stderr").await?;
    output(status, stdout, stderr, operation, limits)
}

pub fn sanitized_diagnostic(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|character| {
            if character.is_control() && !matches!(character, '\n' | '\r' | '\t') {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

struct BoundedOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn read_bounded(mut reader: impl Read, limit: usize) -> io::Result<BoundedOutput> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut exceeded = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        exceeded |= read > remaining;
    }
    Ok(BoundedOutput { bytes, exceeded })
}

async fn read_bounded_async(
    mut reader: impl AsyncRead + Unpin + Send + 'static,
    limit: usize,
) -> io::Result<BoundedOutput> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut exceeded = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        bytes.extend_from_slice(&buffer[..read.min(remaining)]);
        exceeded |= read > remaining;
    }
    Ok(BoundedOutput { bytes, exceeded })
}

fn join_sync_reader(
    reader: thread::JoinHandle<io::Result<BoundedOutput>>,
    operation: &str,
    stream: &'static str,
) -> Result<BoundedOutput, CommandCaptureError> {
    reader
        .join()
        .map_err(|_| CommandCaptureError::ReaderPanicked {
            operation: operation.to_string(),
            stream,
        })?
        .map_err(|source| CommandCaptureError::Read {
            operation: operation.to_string(),
            stream,
            source,
        })
}

async fn join_async_reader(
    reader: tokio::task::JoinHandle<io::Result<BoundedOutput>>,
    operation: &str,
    stream: &'static str,
) -> Result<BoundedOutput, CommandCaptureError> {
    reader
        .await
        .map_err(|error| CommandCaptureError::ReaderTask {
            operation: operation.to_string(),
            stream,
            message: error.to_string(),
        })?
        .map_err(|source| CommandCaptureError::Read {
            operation: operation.to_string(),
            stream,
            source,
        })
}

fn output(
    status: ExitStatus,
    stdout: BoundedOutput,
    stderr: BoundedOutput,
    operation: &str,
    limits: CaptureLimits,
) -> Result<Output, CommandCaptureError> {
    if stdout.exceeded {
        return Err(CommandCaptureError::OutputLimit {
            operation: operation.to_string(),
            stream: "stdout",
            limit: limits.stdout_bytes,
        });
    }
    if stderr.exceeded {
        return Err(CommandCaptureError::OutputLimit {
            operation: operation.to_string(),
            stream: "stderr",
            limit: limits.stderr_bytes,
        });
    }
    Ok(Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    })
}

pub(crate) fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

pub(crate) fn terminate_sync(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;

        let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

async fn terminate_async(child: &mut tokio::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;

        if let Some(id) = child.id() {
            let _ = killpg(Pid::from_raw(id as i32), Signal::SIGKILL);
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const TEST_LIMITS: CaptureLimits = CaptureLimits::new(Duration::from_millis(100), 512, 512);

    #[test]
    fn synchronous_capture_bounds_time_and_output() {
        let started = Instant::now();
        let timeout = capture_output(
            Command::new("sh").args(["-c", "sleep 5"]),
            "run synchronous test command",
            TEST_LIMITS,
        )
        .unwrap_err();
        assert!(timeout.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));

        let excess = capture_output(
            Command::new("sh").args(["-c", "head -c 2048 /dev/zero"]),
            "run synchronous test command",
            TEST_LIMITS,
        )
        .unwrap_err();
        assert!(excess.to_string().contains("stdout exceeded 512 bytes"));
    }

    #[tokio::test]
    async fn asynchronous_capture_bounds_time_and_output() {
        let started = Instant::now();
        let timeout = capture_output_async(
            tokio::process::Command::new("sh").args(["-c", "sleep 5"]),
            "run asynchronous test command",
            TEST_LIMITS,
        )
        .await
        .unwrap_err();
        assert!(timeout.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));

        let excess = capture_output_async(
            tokio::process::Command::new("sh").args(["-c", "head -c 2048 /dev/zero"]),
            "run asynchronous test command",
            TEST_LIMITS,
        )
        .await
        .unwrap_err();
        assert!(excess.to_string().contains("stdout exceeded 512 bytes"));
    }

    #[test]
    fn diagnostics_replace_unsafe_control_characters() {
        assert_eq!(sanitized_diagnostic(b"bad\x01value\n"), "bad value");
    }
}
