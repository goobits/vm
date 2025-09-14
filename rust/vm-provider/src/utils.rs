use anyhow::Result;
use duct::cmd;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};

/// Executes a command, streaming its output to the console.
pub fn stream_command<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    let reader = cmd(command, args).stderr_to_stdout().reader()?;

    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next() {
        println!("{}", line?);
    }

    Ok(())
}

/// Checks if a command-line tool is available in the system's PATH.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which::which(tool_name).is_ok()
}
