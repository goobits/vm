// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};

// External crates
use crate::error::Result;
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
    let reader = with_buildkit(command, args).stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();
    for line in lines {
        info!("{}", line?);
    }
    Ok(())
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
    mut parser: Option<Box<dyn ProgressParser>>,
) -> Result<()> {
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

/// Checks if a command-line tool is available in the system's PATH.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which(tool_name).is_ok()
}
