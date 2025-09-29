// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};

// External crates
use crate::error::Result;
use duct::cmd;
use which::which;

/// Trait for progress parsers (defined here to avoid circular dependencies)
pub trait ProgressParser: Send + Sync {
    /// Parses a single line of output.
    fn parse_line(&mut self, line: &str);
    /// Marks the progress as finished.
    fn finish(&self);
}

/// The original simple command streamer for backward compatibility.
pub fn stream_command<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    let reader = cmd(command, args).stderr_to_stdout().reader()?;
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
    let reader = cmd(command, args).stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();

    for line in lines {
        let line = line?;
        if let Some(ref mut p) = parser {
            p.parse_line(&line);
        } else {
            println!("{}", line);
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
