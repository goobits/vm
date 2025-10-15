// Standard library
use std::ffi::OsStr;
use std::io::{BufRead, BufReader};

// External crates
use crate::error::Result;
use duct::cmd;
use tracing::info;
use which::which;

/// A trait for parsing the output of a command to provide progress feedback.
///
/// This trait is implemented by types that can parse lines of output from a running command
/// and translate them into a user-facing progress indicator, such as a progress bar.
pub trait ProgressParser: Send + Sync {
    /// Parses a single line of output from the command.
    ///
    /// # Arguments
    ///
    /// * `line` - A string slice containing a single line of output from the command.
    fn parse_line(&mut self, line: &str);

    /// Called when the command has finished executing.
    ///
    /// This method can be used to finalize the progress indication, for example, by marking
    /// a progress bar as complete.
    fn finish(&self);
}

/// Streams the output of a command to the logger without any special handling.
///
/// This function executes a command and streams its standard output and standard error
/// to the `info` log target. It is a simple way to run a command and display its
/// output to the user.
///
/// # Arguments
///
/// * `command` - The command to be executed.
/// * `args` - A slice of arguments to pass to the command.
///
/// # Returns
///
/// A `Result` which is `Ok` if the command executes successfully, or an `Err` with a
/// `VmError` if the command fails to execute or returns a non-zero exit code.
pub fn stream_command<A: AsRef<OsStr>>(command: &str, args: &[A]) -> Result<()> {
    let reader = cmd(command, args).stderr_to_stdout().reader()?;
    let lines = BufReader::new(reader).lines();
    for line in lines {
        info!("{}", line?);
    }
    Ok(())
}

/// Streams the output of a command, optionally parsing it for progress feedback.
///
/// This function executes a command and streams its output. If a `parser` is provided,
/// each line of output is passed to the parser. Otherwise, the output is logged to the
/// `info` target.
///
/// # Arguments
///
/// * `command` - The command to be executed.
/// * `args` - A slice of arguments to pass to the command.
/// * `parser` - An optional `ProgressParser` to handle the command's output.
///
/// # Returns
///
/// A `Result` which is `Ok` if the command executes successfully, or an `Err` with a
/// `VmError` if the command fails.
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
            info!("{}", line);
        }
    }

    if let Some(p) = parser {
        p.finish();
    }

    Ok(())
}

/// Checks if a command-line tool is available in the system's PATH.
///
/// This function is useful for verifying that a required dependency is installed before
/// attempting to use it.
///
/// # Arguments
///
/// * `tool_name` - The name of the command-line tool to check for.
///
/// # Returns
///
/// `true` if the tool is found in the system's PATH, `false` otherwise.
pub fn is_tool_installed(tool_name: &str) -> bool {
    which(tool_name).is_ok()
}
