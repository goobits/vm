// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::thread;
use std::time::Duration;

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

/// Helper to parse stdout lines with optional parser
fn parse_stdout_lines(stdout: &[u8], parser: &mut Option<Box<dyn ProgressParser>>) {
    if let Ok(stdout_str) = String::from_utf8(stdout.to_vec()) {
        for line in stdout_str.lines() {
            if let Some(ref mut p) = parser {
                p.parse_line(line);
            } else {
                info!("{}", line);
            }
        }
    }
}

/// The original simple command streamer for backward compatibility.
pub fn stream_command<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    stream_command_with_timeout(command, args, None)
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
        println!("{}", line?);
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
            // With timeout
            let handle = with_buildkit(command, args)
                .stderr_to_stdout()
                .stdout_capture()
                .unchecked()
                .start()
                .map_err(|e| {
                    VmError::Internal(format!("Failed to start command '{}': {}", full_command, e))
                })?;

            let start = std::time::Instant::now();
            let timeout = Duration::from_secs(secs);

            loop {
                if start.elapsed() >= timeout {
                    let _ = handle.kill();
                    return Err(VmError::Timeout(format!(
                        "Command timed out after {}s: {}\n\nTo debug, try running manually:\n  {}",
                        secs, full_command, full_command
                    )));
                }

                match handle.try_wait() {
                    Ok(Some(output)) => {
                        // Process finished - parse output
                        parse_stdout_lines(&output.stdout, &mut parser);

                        if let Some(p) = parser {
                            p.finish();
                        }

                        if !output.status.success() {
                            // Capture the actual output for error reporting
                            let stdout_str = String::from_utf8_lossy(&output.stdout);
                            // Show last 50 lines of output for context
                            let error_context: Vec<&str> = stdout_str
                                .lines()
                                .rev()
                                .take(50)
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev()
                                .collect();

                            return Err(VmError::Internal(format!(
                                "Command failed with exit code {:?}: {}\n\nOutput (last 50 lines):\n{}",
                                output.status.code(),
                                full_command,
                                error_context.join("\n")
                            )));
                        }
                        return Ok(());
                    }
                    Ok(None) => {
                        thread::sleep(Duration::from_millis(100));
                    }
                    Err(e) => {
                        return Err(VmError::Internal(format!(
                            "Error waiting for command '{}': {}",
                            full_command, e
                        )));
                    }
                }
            }
        }
    }
}

/// Checks if a command-line tool is available in the system's PATH.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which(tool_name).is_ok()
}
