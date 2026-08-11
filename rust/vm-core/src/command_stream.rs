// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

// External crates
use crate::error::{Result, VmError};
use duct::cmd;
use tracing::info;
use which::which;

/// Trait for progress parsers (defined here to avoid circular dependencies)
pub trait ProgressParser: Send + Sync {
    /// Parses a single line of output.
    fn parse_line(&mut self, line: &str);
    /// Marks the progress as finished.
    fn finish(&self);
}

/// Helper to enable BuildKit for Docker commands
/// This provides 40-60% faster builds through parallel layer processing and cache mounts
fn with_buildkit<A: AsRef<OsStr>>(command: &str, args: &[A]) -> duct::Expression {
    let mut cmd_builder = cmd(command, args);

    if command == "docker" {
        cmd_builder = cmd_builder
            .env("DOCKER_BUILDKIT", "1")
            .env("COMPOSE_DOCKER_CLI_BUILD", "1")
            .env("BUILDKIT_PROGRESS", "plain");
    }

    cmd_builder
}

/// The original simple command streamer for backward compatibility.
pub fn stream_command<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    stream_command_with_timeout(command, args, None)
}

/// Stream command output with additional environment variables.
pub fn stream_command_with_env<A, K, V>(command: &str, args: &[A], envs: &[(K, V)]) -> Result<()>
where
    A: AsRef<OsStr>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut expr = with_buildkit(command, args);
    for (key, value) in envs {
        expr = expr.env(key, value);
    }

    let reader = expr.stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();

    for line in lines {
        info!("{}", line?);
    }

    Ok(())
}

/// Stream command output with optional timeout (in seconds).
/// If timeout is None, command runs indefinitely.
/// If timeout is exceeded, returns VmError with the full command for debugging.
pub fn stream_command_with_timeout<A: AsRef<OsStr>>(
    command: &str,
    args: &[A],
    timeout_secs: Option<u64>,
) -> Result<()> {
    // Delegate to the progress variant with no parser
    // This eliminates code duplication while keeping the same behavior
    stream_command_with_progress_and_timeout(command, args, None, timeout_secs)
}

/// Stream command output directly to stdout, bypassing the logging system.
/// Use this for long-running commands where user needs progress feedback.
pub fn stream_command_visible<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    let reader = with_buildkit(command, args).stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();
    for line in lines {
        crate::vm_println!("{}", line?);
    }
    Ok(())
}

/// Stream command output directly to stdout with additional environment variables.
pub fn stream_command_visible_with_env<A, K, V>(
    command: &str,
    args: &[A],
    envs: &[(K, V)],
) -> Result<()>
where
    A: AsRef<OsStr>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let mut expr = with_buildkit(command, args);
    for (key, value) in envs {
        expr = expr.env(key, value);
    }

    let reader = expr.stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();
    for line in lines {
        crate::vm_println!("{}", line?);
    }
    Ok(())
}

/// Stream command output with optional progress parsing
pub fn stream_command_with_progress<A: AsRef<OsStr>>(
    command: &str,
    args: &[A],
    parser: Option<Box<dyn ProgressParser>>,
) -> Result<()> {
    stream_command_with_progress_and_timeout(command, args, parser, None)
}

/// Stream command output with optional progress parsing and timeout
pub fn stream_command_with_progress_and_timeout<A: AsRef<OsStr>>(
    command: &str,
    args: &[A],
    mut parser: Option<Box<dyn ProgressParser>>,
    timeout_secs: Option<u64>,
) -> Result<()> {
    let full_command = format!(
        "{} {}",
        command,
        args.iter()
            .map(|a| a.as_ref().to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );

    match timeout_secs {
        None => {
            // No timeout - original behavior
            let reader = with_buildkit(command, args).stderr_to_stdout().reader()?;
            let lines = BufReader::new(reader).lines();

            for line in lines {
                let line = line?;
                if let Some(ref mut p) = parser {
                    p.parse_line(&line);
                } else {
                    info!("{}", line);
                }
            }

            if let Some(p) = parser {
                p.finish();
            }

            Ok(())
        }
        Some(secs) => {
            run_with_timeout(
                command,
                args,
                &mut parser,
                Duration::from_secs(secs),
                &full_command,
            )?;
            if let Some(parser) = parser.take() {
                parser.finish();
            }
            Ok(())
        }
    }
}

enum StreamMessage {
    Line(String),
    Error(String),
}

fn run_with_timeout<A: AsRef<OsStr>>(
    executable: &str,
    args: &[A],
    parser: &mut Option<Box<dyn ProgressParser>>,
    timeout: Duration,
    full_command: &str,
) -> Result<()> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_buildkit_environment(&mut command, executable);
    let mut child = command.spawn()?;
    let Some(stdout) = child.stdout.take() else {
        terminate_and_wait(&mut child);
        return Err(VmError::Internal(format!(
            "Could not capture stdout for {full_command}"
        )));
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_and_wait(&mut child);
        return Err(VmError::Internal(format!(
            "Could not capture stderr for {full_command}"
        )));
    };
    let (sender, receiver) = mpsc::channel();
    let stdout_reader = spawn_reader(stdout, sender.clone());
    let stderr_reader = spawn_reader(stderr, sender);
    let deadline = Instant::now() + timeout;
    let mut recent = std::collections::VecDeque::with_capacity(50);
    let mut stream_error = None;

    loop {
        drain_message(
            &receiver,
            parser,
            &mut recent,
            &mut stream_error,
            Duration::from_millis(50).min(deadline.saturating_duration_since(Instant::now())),
        );

        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                terminate_and_wait(&mut child);
                join_readers(stdout_reader, stderr_reader, full_command)?;
                return Err(error.into());
            }
        };
        if let Some(status) = status {
            join_readers(stdout_reader, stderr_reader, full_command)?;
            drain_remaining(&receiver, parser, &mut recent, &mut stream_error);
            if let Some(error) = stream_error {
                return Err(VmError::Internal(format!(
                    "Command output failed for {full_command}: {error}"
                )));
            }
            if status.success() {
                return Ok(());
            }
            return Err(VmError::Command(format!(
                "Command failed with {status}: {full_command}\n\nOutput (last 50 lines):\n{}",
                recent.iter().cloned().collect::<Vec<_>>().join("\n")
            )));
        }

        if Instant::now() >= deadline {
            terminate_and_wait(&mut child);
            join_readers(stdout_reader, stderr_reader, full_command)?;
            drain_remaining(&receiver, parser, &mut recent, &mut stream_error);
            return Err(VmError::Timeout(format!(
                "Command timed out after {}s: {}\n\nOutput (last 50 lines):\n{}\n\nTo debug, try running manually:\n  {}",
                timeout.as_secs(),
                full_command,
                recent.iter().cloned().collect::<Vec<_>>().join("\n"),
                full_command
            )));
        }
    }
}

fn apply_buildkit_environment(command: &mut Command, executable: &str) {
    if executable == "docker" {
        command
            .env("DOCKER_BUILDKIT", "1")
            .env("COMPOSE_DOCKER_CLI_BUILD", "1")
            .env("BUILDKIT_PROGRESS", "plain");
    }
}

fn spawn_reader<R: std::io::Read + Send + 'static>(
    reader: R,
    sender: Sender<StreamMessage>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines() {
            let message = match line {
                Ok(line) => StreamMessage::Line(line),
                Err(error) => StreamMessage::Error(error.to_string()),
            };
            if sender.send(message).is_err() {
                break;
            }
        }
    })
}

fn drain_message(
    receiver: &Receiver<StreamMessage>,
    parser: &mut Option<Box<dyn ProgressParser>>,
    recent: &mut std::collections::VecDeque<String>,
    stream_error: &mut Option<String>,
    wait: Duration,
) {
    match receiver.recv_timeout(wait) {
        Ok(message) => record_message(message, parser, recent, stream_error),
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
    }
}

fn drain_remaining(
    receiver: &Receiver<StreamMessage>,
    parser: &mut Option<Box<dyn ProgressParser>>,
    recent: &mut std::collections::VecDeque<String>,
    stream_error: &mut Option<String>,
) {
    while let Ok(message) = receiver.try_recv() {
        record_message(message, parser, recent, stream_error);
    }
}

fn record_message(
    message: StreamMessage,
    parser: &mut Option<Box<dyn ProgressParser>>,
    recent: &mut std::collections::VecDeque<String>,
    stream_error: &mut Option<String>,
) {
    match message {
        StreamMessage::Line(line) => {
            if recent.len() == 50 {
                recent.pop_front();
            }
            recent.push_back(line.clone());
            if let Some(parser) = parser.as_deref_mut() {
                parser.parse_line(&line);
            } else {
                info!("{}", line);
            }
        }
        StreamMessage::Error(error) => *stream_error = Some(error),
    }
}

fn terminate_and_wait(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn join_readers(
    stdout: thread::JoinHandle<()>,
    stderr: thread::JoinHandle<()>,
    full_command: &str,
) -> Result<()> {
    let stdout_result = stdout.join();
    let stderr_result = stderr.join();
    if stdout_result.is_err() || stderr_result.is_err() {
        return Err(VmError::Internal(format!(
            "Output reader panicked while running {full_command}"
        )));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::stream_command_with_progress_and_timeout;
    use std::time::{Duration, Instant};

    #[test]
    fn timeout_reaps_the_command_before_returning() {
        let started = Instant::now();
        let error = stream_command_with_progress_and_timeout(
            "/bin/sh",
            &["-c", "while :; do printf 'waiting\\n'; sleep 0.05; done"],
            None,
            Some(1),
        )
        .unwrap_err();

        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(error.to_string().contains("timed out"));
    }
}

/// Checks if a command-line tool is available in the system's PATH.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which(tool_name).is_ok()
}
