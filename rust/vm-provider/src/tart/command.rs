use std::ffi::{OsStr, OsString};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(super) struct TartCommand {
    tart_home: Option<String>,
}

impl TartCommand {
    pub(super) fn new(tart_home: Option<String>) -> Self {
        Self { tart_home }
    }

    pub(super) fn command(&self) -> Command {
        let mut command = Command::new("tart");
        if let Some(tart_home) = &self.tart_home {
            command.env("TART_HOME", tart_home);
        }
        command
    }

    pub(super) fn expr<A: AsRef<OsStr>>(&self, args: &[A]) -> duct::Expression {
        let args: Vec<OsString> = args.iter().map(|arg| arg.as_ref().to_os_string()).collect();
        let mut expr = duct::cmd("tart", args);
        if let Some(tart_home) = &self.tart_home {
            expr = expr.env("TART_HOME", tart_home);
        }
        expr
    }

    pub(super) fn exec_probe<I, S>(&self, instance: &str, args: I, timeout: Duration) -> bool
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let Ok(mut child) = self
            .command()
            .arg("exec")
            .arg(instance)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };

        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TartCommand;

    #[test]
    fn command_applies_tart_home_when_present() {
        let command = TartCommand::new(Some("/tmp/tart-home".to_string())).command();
        let tart_home = command
            .get_envs()
            .find_map(|(key, value)| (key == "TART_HOME").then_some(value))
            .flatten();

        assert_eq!(tart_home, Some(std::ffi::OsStr::new("/tmp/tart-home")));
    }

    #[test]
    fn command_omits_tart_home_when_absent() {
        let command = TartCommand::new(None).command();
        assert!(!command.get_envs().any(|(key, _)| key == "TART_HOME"));
    }
}
