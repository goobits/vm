//! Exit status of a command executed in a guest.

use std::process::{Command, ExitStatus};

use vm_core::error::{Result, VmError};

/// A shell-compatible exit code from the guest command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GuestExit {
    code: i32,
}

/// Captured guest streams and exact child status for framed fleet execution.
#[derive(Debug)]
pub struct GuestOutput {
    pub status: GuestExit,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl GuestExit {
    pub fn new(code: i32) -> Self {
        Self { code }
    }

    pub fn code(self) -> i32 {
        self.code
    }

    pub fn success(self) -> bool {
        self.code == 0
    }

    pub fn from_status(status: ExitStatus) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return Self::new(128 + signal);
            }
        }
        Self::new(status.code().unwrap_or(1))
    }
}

/// Run the transport with inherited streams so guest bytes remain untouched.
pub(crate) fn run_guest_command(command: &mut Command) -> Result<GuestExit> {
    let status = command
        .status()
        .map_err(|error| VmError::Provider(format!("Failed to launch guest command: {error}")))?;
    Ok(GuestExit::from_status(status))
}

pub(crate) fn capture_guest_command(command: &mut Command) -> Result<GuestOutput> {
    let output = command
        .output()
        .map_err(|error| VmError::Provider(format!("Failed to launch guest command: {error}")))?;
    Ok(GuestOutput {
        status: GuestExit::from_status(output.status),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::{capture_guest_command, run_guest_command, GuestExit};
    use std::fs::File;
    use std::process::{Command, Stdio};

    #[test]
    fn preserves_output_bytes_and_exit_status() {
        let directory = tempfile::tempdir().unwrap();
        let stdout = directory.path().join("stdout");
        let stderr = directory.path().join("stderr");
        let mut command = Command::new("sh");
        command
            .args(["-c", "printf '\\377out'; printf '\\376err' >&2; exit 42"])
            .stdout(Stdio::from(File::create(&stdout).unwrap()))
            .stderr(Stdio::from(File::create(&stderr).unwrap()));

        assert_eq!(run_guest_command(&mut command).unwrap(), GuestExit::new(42));
        assert_eq!(std::fs::read(stdout).unwrap(), b"\xffout");
        assert_eq!(std::fs::read(stderr).unwrap(), b"\xfeerr");
    }

    #[test]
    fn signal_exit_uses_shell_convention() {
        let mut command = Command::new("sh");
        command.args(["-c", "kill -TERM $$"]);
        assert_eq!(
            run_guest_command(&mut command).unwrap(),
            GuestExit::new(143)
        );
    }

    #[test]
    fn missing_transport_is_a_launch_error() {
        let mut command = Command::new("/missing/vm-exec-transport");
        assert!(run_guest_command(&mut command).is_err());
    }

    #[test]
    fn capture_preserves_binary_streams_and_status() {
        let output = capture_guest_command(
            Command::new("sh").args(["-c", "printf '\\377out'; printf '\\376err' >&2; exit 42"]),
        )
        .unwrap();
        assert_eq!(output.status, GuestExit::new(42));
        assert_eq!(output.stdout, b"\xffout");
        assert_eq!(output.stderr, b"\xfeerr");
    }
}
