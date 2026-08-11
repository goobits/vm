use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use vm_config::config::VmConfig;
use vm_core::error::Result;

use crate::tart_storage as storage;

#[derive(Clone, Debug)]
pub struct TartCommand {
    tart_home: Option<PathBuf>,
}

impl TartCommand {
    pub fn new(tart_home: Option<PathBuf>) -> Self {
        Self { tart_home }
    }

    /// Resolve the command context from explicit config, then `TART_HOME`.
    pub fn from_config(config: Option<&VmConfig>) -> Self {
        Self::new(storage::configured_home(config))
    }

    /// Resolve a project command context, including a recorded or narrowly
    /// recovered home for existing instances.
    pub fn for_project(config: &VmConfig, project: &str) -> Result<Self> {
        Ok(Self::new(storage::resolve_project_home(config, project)?))
    }

    pub fn home(&self) -> Option<&Path> {
        self.tart_home.as_deref()
    }

    pub fn remember_instance(&self, instance: &str) -> Result<()> {
        if let Some(home) = self.home() {
            storage::remember_instance(instance, home)?;
        }
        Ok(())
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new("tart");
        self.configure(&mut command);
        command
    }

    pub fn configure(&self, command: &mut Command) {
        if let Some(tart_home) = &self.tart_home {
            command.env("TART_HOME", tart_home);
        }
    }

    pub fn expr<A: AsRef<OsStr>>(&self, args: &[A]) -> duct::Expression {
        let args: Vec<OsString> = args.iter().map(|arg| arg.as_ref().to_os_string()).collect();
        self.with_env(duct::cmd("tart", args))
    }

    pub fn with_env(&self, mut expr: duct::Expression) -> duct::Expression {
        if let Some(tart_home) = &self.tart_home {
            expr = expr.env("TART_HOME", tart_home);
        }
        expr
    }

    pub fn exec_probe<I, S>(&self, instance: &str, args: I, timeout: Duration) -> bool
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

    pub fn ip_address(&self, instance: &str, timeout: Duration) -> Option<IpAddr> {
        let Ok(mut child) = self
            .command()
            .args(["ip", instance, "--wait", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            return None;
        };
        let mut stdout = child.stdout.take()?;
        let deadline = Instant::now() + timeout;

        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(100));
                }
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
            }
        }

        let mut output = String::new();
        stdout.read_to_string(&mut output).ok()?;
        parse_ip_address(&output)
    }
}

fn parse_ip_address(output: &str) -> Option<IpAddr> {
    output
        .split_whitespace()
        .find_map(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::{parse_ip_address, TartCommand};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::path::PathBuf;

    #[test]
    fn command_applies_tart_home_when_present() {
        let command = TartCommand::new(Some(PathBuf::from("/tmp/tart-home"))).command();
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

    #[test]
    fn parses_tart_ip_output() {
        assert_eq!(
            parse_ip_address("192.168.64.37\n"),
            Some(IpAddr::V4(Ipv4Addr::new(192, 168, 64, 37)))
        );
        assert_eq!(
            parse_ip_address("fd00::37\n"),
            Some(IpAddr::V6("fd00::37".parse::<Ipv6Addr>().unwrap()))
        );
        assert_eq!(parse_ip_address(""), None);
    }
}
