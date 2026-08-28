use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use vm_config::config::VmConfig;
use vm_core::error::Result;
#[cfg(feature = "tart")]
use vm_core::error::VmError;

use super::storage;

#[derive(Clone, Debug)]
pub(crate) struct TartCommand {
    tart_home: Option<PathBuf>,
    config_path: Option<PathBuf>,
}

impl TartCommand {
    pub fn new(tart_home: Option<PathBuf>) -> Self {
        Self {
            tart_home,
            config_path: None,
        }
    }

    /// Resolve the command context from explicit config, then `TART_HOME`.
    pub fn from_config(config: Option<&VmConfig>) -> Self {
        Self {
            tart_home: storage::configured_home(config),
            config_path: config
                .and_then(VmConfig::owning_config_path)
                .map(Path::to_path_buf),
        }
    }

    /// Resolve a project command context, including a recorded or narrowly
    /// recovered home for existing instances.
    pub fn for_project(config: &VmConfig, project: &str) -> Result<Self> {
        Ok(Self {
            tart_home: storage::resolve_project_home(config, project)?,
            config_path: config.owning_config_path().map(Path::to_path_buf),
        })
    }

    pub fn home(&self) -> Option<&Path> {
        self.tart_home.as_deref()
    }

    pub fn remember_instance(&self, instance: &str) -> Result<()> {
        storage::remember_instance(instance, self.home(), self.config_path.as_deref())
    }

    pub fn instance_config_path(&self, instance: &str) -> Result<Option<PathBuf>> {
        storage::instance_config_path(instance)
    }

    pub fn managed_instances(&self) -> Result<std::collections::BTreeSet<String>> {
        storage::managed_instances()
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
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        };
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

#[cfg(feature = "tart")]
pub(crate) fn validate_environment() -> Result<()> {
    let output = TartCommand::new(None)
        .command()
        .arg("--version")
        .output()
        .map_err(|error| VmError::Dependency(format!("tart is not installed: {error}")))?;
    if !output.status.success() {
        return Err(VmError::Provider("tart is not available".to_string()));
    }

    let version = String::from_utf8_lossy(&output.stdout);
    if tart_version_number(&version) == Some("2.35.0") {
        Err(VmError::Provider(
            "known incompatible Tart release 2.35.0".to_string(),
        ))
    } else {
        Ok(())
    }
}

#[cfg(any(feature = "tart", test))]
fn tart_version_number(output: &str) -> Option<&str> {
    output.split_whitespace().find(|value| {
        value
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit())
    })
}

fn parse_ip_address(output: &str) -> Option<IpAddr> {
    output
        .split_whitespace()
        .find_map(|value| value.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::{parse_ip_address, tart_version_number, TartCommand, VmConfig};
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
    fn command_tracks_project_config_ownership() {
        let config = VmConfig {
            source_path: Some(PathBuf::from("/tmp/project/vm.yaml")),
            ..Default::default()
        };
        let command = TartCommand::from_config(Some(&config));

        assert_eq!(
            command.config_path.as_deref(),
            Some(std::path::Path::new("/tmp/project/vm.yaml"))
        );
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

    #[test]
    fn parses_tart_version_without_assuming_a_prefix() {
        assert_eq!(tart_version_number("tart 2.32.1\n"), Some("2.32.1"));
        assert_eq!(tart_version_number("2.35.0\n"), Some("2.35.0"));
        assert_eq!(tart_version_number("tart unknown\n"), None);
    }
}
