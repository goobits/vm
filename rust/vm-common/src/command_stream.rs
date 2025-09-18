// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};

// External crates
use anyhow::Result;
use duct::cmd;
use which::which;

/// The original simple command streamer for backward compatibility.
pub fn stream_command<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    let reader = cmd(command, args).stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();
    for line in lines {
        println!("{}", line?);
    }
    Ok(())
}

/// Checks if a command-line tool is available in the system's PATH.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which(tool_name).is_ok()
}
