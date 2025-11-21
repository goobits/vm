// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
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
            // With timeout - stream output in real-time using a thread
            let start = std::time::Instant::now();
            let timeout = Duration::from_secs(secs);

            // Convert args to owned strings for thread safety
            let command_owned = command.to_string();
            let args_owned: Vec<String> = args
                .iter()
                .map(|a| a.as_ref().to_string_lossy().to_string())
                .collect();

            // Shared state for collecting output and tracking completion
            let output_lines = Arc::new(Mutex::new(Vec::new()));
            let output_lines_clone = Arc::clone(&output_lines);

            // Spawn thread to stream output
            let parser_arc = Arc::new(Mutex::new(parser));
            let parser_clone = Arc::clone(&parser_arc);

            let handle = thread::spawn(move || {
                let reader = with_buildkit(&command_owned, &args_owned)
                    .stderr_to_stdout()
                    .reader()?;
                let lines = BufReader::new(reader).lines();

                for line in lines {
                    let line = line?;

                    // Store for error reporting
                    if let Ok(mut buf) = output_lines_clone.lock() {
                        buf.push(line.clone());
                    }

                    // Parse/log the line
                    parse_or_log_line(&parser_clone, &line);
                }

                Ok::<(), std::io::Error>(())
            });

            // Monitor for timeout
            loop {
                if start.elapsed() >= timeout {
                    let error_context = get_last_n_lines(&output_lines, 50);
                    return Err(VmError::Timeout(format!(
                        "Command timed out after {}s: {}\n\nOutput (last 50 lines):\n{}\n\nTo debug, try running manually:\n  {}",
                        secs, full_command, error_context.join("\n"), full_command
                    )));
                }

                // Check if thread finished
                if handle.is_finished() {
                    match handle.join() {
                        Ok(Ok(())) => {
                            // Success - finish parser
                            finish_parser(&parser_arc);
                            return Ok(());
                        }
                        Ok(Err(_e)) => {
                            // IO error - show context
                            let error_context = get_last_n_lines(&output_lines, 50);
                            return Err(VmError::Internal(format!(
                                "Command failed: {}\n\nOutput (last 50 lines):\n{}",
                                full_command,
                                error_context.join("\n")
                            )));
                        }
                        Err(_) => {
                            return Err(VmError::Internal(format!(
                                "Thread panicked while running command: {}",
                                full_command
                            )));
                        }
                    }
                }

                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// Checks if a command-line tool is available in the system's PATH.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which(tool_name).is_ok()
}

/// Helper to parse or log a line based on parser availability
fn parse_or_log_line(parser_arc: &Arc<Mutex<Option<Box<dyn ProgressParser>>>>, line: &str) {
    let Ok(mut p) = parser_arc.lock() else {
        return;
    };

    match p.as_mut() {
        Some(parser) => parser.parse_line(line),
        None => info!("{}", line),
    }
}

/// Helper to finish the parser if available
fn finish_parser(parser_arc: &Arc<Mutex<Option<Box<dyn ProgressParser>>>>) {
    let Ok(mut p) = parser_arc.lock() else {
        return;
    };

    if let Some(parser) = p.take() {
        parser.finish();
    }
}

/// Helper to get the last N lines from the output buffer
fn get_last_n_lines(output_lines: &Arc<Mutex<Vec<String>>>, n: usize) -> Vec<String> {
    let Ok(buf) = output_lines.lock() else {
        return Vec::new();
    };

    buf.iter()
        .rev()
        .take(n)
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}
