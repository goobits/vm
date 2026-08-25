use std::fs;
use std::future::Future;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, ExitCode, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use tracing::Instrument;
use vm_packages::sha256_hex;

pub const POLL_INTERVAL: Duration = Duration::from_secs(5);
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(300);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
const MAX_COMMAND_STDOUT: usize = 16 * 1024 * 1024;
const MAX_COMMAND_STDERR: usize = 64 * 1024;

pub struct JobMonitor {
    operation: &'static str,
    failed_id: Option<String>,
    attempts: u32,
}

impl JobMonitor {
    pub fn new(operation: &'static str) -> Self {
        Self {
            operation,
            failed_id: None,
            attempts: 0,
        }
    }

    pub fn succeeded(&mut self, job_id: &str) {
        if self.failed_id.as_deref() == Some(job_id) {
            tracing::info!(
                operation = self.operation,
                job_id,
                failed_attempts = self.attempts,
                outcome = "recovered",
                "package job recovered"
            );
        }
        self.reset();
    }

    pub fn failed(&mut self, job_id: &str, error: &impl std::fmt::Debug) -> Duration {
        if self.failed_id.as_deref() == Some(job_id) {
            self.attempts = self.attempts.saturating_add(1);
        } else {
            self.failed_id = Some(job_id.to_string());
            self.attempts = 1;
        }

        let exponent = self.attempts.saturating_sub(1).min(6);
        let delay = Duration::from_secs(
            POLL_INTERVAL
                .as_secs()
                .saturating_mul(1_u64 << exponent)
                .min(MAX_RETRY_INTERVAL.as_secs()),
        );
        if self.attempts.is_power_of_two() {
            tracing::error!(
                operation = self.operation,
                job_id,
                failed_attempts = self.attempts,
                retry_seconds = delay.as_secs(),
                error = ?error,
                "package job failed"
            );
        }
        delay
    }

    fn reset(&mut self) {
        self.failed_id = None;
        self.attempts = 0;
    }
}

pub struct QueueMonitor {
    operation: &'static str,
    unavailable: bool,
}

impl QueueMonitor {
    pub fn new(operation: &'static str) -> Self {
        Self {
            operation,
            unavailable: false,
        }
    }

    pub fn available(&mut self) {
        if std::mem::take(&mut self.unavailable) {
            tracing::info!(operation = self.operation, "package queue access recovered");
        }
    }

    pub fn unavailable(&mut self, error: &impl std::fmt::Debug) {
        if !std::mem::replace(&mut self.unavailable, true) {
            tracing::warn!(
                operation = self.operation,
                error = ?error,
                "package queue unavailable"
            );
        }
    }
}

pub async fn worker_main<F>(component: &'static str, worker: F) -> ExitCode
where
    F: Future<Output = Result<()>>,
{
    let span = tracing::info_span!("package_worker", component);
    async move {
        match worker.await {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                tracing::error!(operation = "run", error = ?error, "package worker stopped");
                ExitCode::FAILURE
            }
        }
    }
    .instrument(span)
    .await
}

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
    run_command_with_limits(
        command,
        operation,
        COMMAND_TIMEOUT,
        MAX_COMMAND_STDOUT,
        MAX_COMMAND_STDERR,
    )
}

fn run_command_with_limits(
    command: &mut Command,
    operation: &str,
    timeout: Duration,
    stdout_limit: usize,
    stderr_limit: usize,
) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to {operation}"))?;
    let stdout = child
        .stdout
        .take()
        .context("command stdout was not captured")?;
    let stderr = child
        .stderr
        .take()
        .context("command stderr was not captured")?;
    let stdout_reader = thread::spawn(move || read_bounded(stdout, stdout_limit));
    let stderr_reader = thread::spawn(move || read_bounded(stderr, stderr_limit));
    let deadline = Instant::now() + timeout;

    let status = loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("failed to wait for {operation}"))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            terminate(&mut child);
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            bail!("failed to {operation}: command timed out after {timeout:?}");
        }
        thread::sleep(Duration::from_millis(50));
    };

    let stdout = join_reader(stdout_reader, operation)?;
    let stderr = join_reader(stderr_reader, operation)?;
    if stdout.exceeded {
        bail!("failed to {operation}: command stdout exceeded {stdout_limit} bytes");
    }
    if stderr.exceeded {
        bail!("failed to {operation}: command stderr exceeded {stderr_limit} bytes");
    }
    let output = Output {
        status,
        stdout: stdout.bytes,
        stderr: stderr.bytes,
    };
    if output.status.success() {
        return Ok(output);
    }
    bail!(
        "failed to {operation}: {}",
        sanitized_diagnostic(&output.stderr)
    )
}

struct BoundedOutput {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn read_bounded(mut reader: impl Read, limit: usize) -> std::io::Result<BoundedOutput> {
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

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<BoundedOutput>>,
    operation: &str,
) -> Result<BoundedOutput> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("failed to {operation}: output reader panicked"))?
        .with_context(|| format!("failed to read output while attempting to {operation}"))
}

fn sanitized_diagnostic(bytes: &[u8]) -> String {
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

fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;

        let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn command_text(command: &mut Command, operation: &str) -> Result<String> {
    Ok(String::from_utf8(run_command(command, operation)?.stdout)?)
}

pub fn authorization_header(token: &str) -> Result<tempfile::NamedTempFile> {
    let mut header = tempfile::NamedTempFile::new()?;
    writeln!(header, "Authorization: Bearer {token}")?;
    Ok(header)
}

pub fn download_bundle(url: &str, token: &str, destination: &Path) -> Result<()> {
    let header = authorization_header(token)?;
    run_command(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location"])
            .arg("--header")
            .arg(format!("@{}", header.path().display()))
            .arg("--output")
            .arg(destination)
            .arg(url),
        "download immutable source bundle",
    )?;
    Ok(())
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

    #[test]
    fn poison_jobs_back_off_and_reset() {
        let mut monitor = JobMonitor::new("test");
        let delays: Vec<_> = (0..8)
            .map(|_| monitor.failed("job-1", &"failure"))
            .collect();
        assert_eq!(
            delays,
            [5, 10, 20, 40, 80, 160, 300, 300].map(Duration::from_secs)
        );

        monitor.succeeded("job-1");
        assert_eq!(monitor.failed("job-1", &"failure"), Duration::from_secs(5));
        assert_eq!(monitor.failed("job-2", &"failure"), Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn commands_time_out() {
        let started = Instant::now();
        let error = run_command_with_limits(
            Command::new("sh").args(["-c", "sleep 5"]),
            "run test command",
            Duration::from_millis(100),
            1024,
            1024,
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn commands_fail_when_output_exceeds_its_limit() {
        let error = run_command_with_limits(
            Command::new("sh").args(["-c", "head -c 2048 /dev/zero"]),
            "run test command",
            Duration::from_secs(1),
            512,
            1024,
        )
        .unwrap_err();
        assert!(error.to_string().contains("stdout exceeded 512 bytes"));
    }

    #[cfg(unix)]
    #[test]
    fn command_errors_replace_unsafe_control_characters() {
        let error = run_command_with_limits(
            Command::new("sh").args(["-c", "printf 'bad\\001value' >&2; exit 1"]),
            "run test command",
            Duration::from_secs(1),
            1024,
            1024,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), "failed to run test command: bad value");
    }
}
