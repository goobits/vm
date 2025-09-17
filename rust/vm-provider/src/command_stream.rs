// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};

// External crates
use anyhow::{Context, Result};
use duct::cmd;
use which::which;

// Internal imports
use crate::progress::{DockerProgressParser, ProgressParser};

/// The original simple command streamer for backward compatibility.
pub fn stream_command<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    let reader = cmd(command, args).stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();
    for line in lines {
        println!("{}", line?);
    }
    Ok(())
}

/// Executes a command, streaming its output to a parser for rich progress.
pub fn stream_command_with_progress<A: AsRef<OsStr>>(
    command: &str,
    args: &[A],
    mut parser: Box<dyn ProgressParser>,
) -> Result<()> {
    let reader = cmd(command, args).stderr_to_stdout().reader()
        .with_context(|| {
            let args_str = args.iter()
                .map(|arg| arg.as_ref().to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            format!("Failed to execute command '{}' with args: {}", command, args_str)
        })?;

    let lines = BufReader::new(reader).lines();

    for line_result in lines {
        let line = line_result?;
        parser.parse_line(&line);
    }

    parser.finish();
    Ok(())
}

/// A convenience function for streaming Docker builds with progress.
pub fn stream_docker_build<A: AsRef<OsStr>>(args: &[A]) -> Result<()> {
    let parser = DockerProgressParser::new();
    stream_command_with_progress("docker", args, Box::new(parser))
}

/// Checks if a command-line tool is available in the system's PATH.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which(tool_name).is_ok()
}
