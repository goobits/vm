use std::path::Path;
use std::process::Command;
use std::time::Duration;

use vm_config::config::ProviderName;
use vm_core::command_stream::{is_tool_installed, stream_command};
use vm_core::error::{Result, VmError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PodmanCompose {
    BuiltIn,
    Standalone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerEngine {
    Docker,
    Podman(PodmanCompose),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComposeRuntime {
    BuiltIn,
    Standalone,
}

pub(crate) struct ComposeInvocation {
    program: String,
    args: Vec<String>,
}

impl ContainerEngine {
    pub fn detect(provider: &ProviderName) -> Result<Self> {
        let executable = provider.as_str();
        if !is_tool_installed(executable) {
            return Err(VmError::Dependency(format!(
                "{} is not installed",
                provider
            )));
        }
        match provider {
            ProviderName::Docker => Ok(Self::Docker),
            ProviderName::Podman => detect_podman_compose().map(Self::Podman),
            _ => Err(VmError::Provider(format!(
                "Provider '{provider}' is not a container engine"
            ))),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman(_) => "podman",
        }
    }

    pub fn executable(self) -> &'static str {
        self.name()
    }

    /// Start a Compose command using the engine's supported invocation form.
    pub fn compose_command(self) -> Command {
        match self.compose_runtime() {
            ComposeRuntime::BuiltIn => {
                let mut command = Command::new(self.executable());
                command.arg("compose");
                command
            }
            ComposeRuntime::Standalone => Command::new("podman-compose"),
        }
    }

    pub(crate) fn compose_runtime(self) -> ComposeRuntime {
        match self {
            Self::Docker | Self::Podman(PodmanCompose::BuiltIn) => ComposeRuntime::BuiltIn,
            Self::Podman(PodmanCompose::Standalone) => ComposeRuntime::Standalone,
        }
    }

    pub fn validate(self) -> Result<()> {
        let executable = self.executable();
        self.validate_executable(executable)
    }

    pub fn validate_executable(self, executable: &str) -> Result<()> {
        let version = Command::new(executable)
            .arg("--version")
            .output()
            .map_err(|_| install_error(self))?;
        if !version.status.success() {
            return Err(install_error(self));
        }

        let output = Command::new(executable).arg("ps").output()?;
        if output.status.success() {
            return Ok(());
        }

        let details = String::from_utf8_lossy(&output.stderr);
        if details.contains("permission denied") {
            return match self {
                Self::Docker => Err(VmError::DockerPermission(
                    "Fix: sudo usermod -aG docker $USER && newgrp docker".to_string(),
                )),
                Self::Podman(_) => Err(VmError::Provider(
                    "Podman permission denied. Ensure you can run: podman ps".to_string(),
                )),
            };
        }

        match self {
            Self::Docker => Err(VmError::DockerNotRunning(
                "Start Docker Desktop or run: sudo systemctl start docker".to_string(),
            )),
            Self::Podman(_) => Err(VmError::Provider(format!(
                "Podman is not working correctly: {details}"
            ))),
        }
    }

    /// Start one managed TCP relay beside a target container.
    pub fn start_tcp_relay(
        self,
        relay_name: &str,
        host_port: u16,
        target_container: &str,
        target_port: u16,
    ) -> Result<String> {
        let (network_name, target_address) = self.container_network(target_container)?;
        let output = Command::new(self.executable())
            .args([
                "run",
                "-d",
                "--rm",
                "--name",
                relay_name,
                &format!("--network={network_name}"),
                "-p",
                &format!("{host_port}:{host_port}"),
                "alpine/socat",
                &format!("tcp-listen:{host_port},fork,reuseaddr"),
                &format!("tcp-connect:{target_address}:{target_port}"),
            ])
            .output()
            .map_err(|error| VmError::general(error, "Failed to start tunnel relay"))?;
        if !output.status.success() {
            return Err(command_error("start tunnel relay", &output.stderr));
        }
        std::thread::sleep(Duration::from_millis(500));
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Resolve the first usable network and address for a container.
    pub fn container_network(self, container_name: &str) -> Result<(String, String)> {
        let output = Command::new(self.executable())
            .args([
                "inspect",
                "--type",
                "container",
                "--format",
                "{{range $name, $network := .NetworkSettings.Networks}}{{$name}} {{$network.IPAddress}}{{println}}{{end}}",
                container_name,
            ])
            .output()
            .map_err(|error| {
                VmError::general(
                    error,
                    format!("Failed to inspect container network for {container_name}"),
                )
            })?;
        if !output.status.success() {
            return Err(command_error("inspect container network", &output.stderr));
        }
        parse_container_network(&String::from_utf8_lossy(&output.stdout), container_name)
    }

    /// Return whether a container is currently running.
    pub fn container_is_running(self, container_id: &str) -> bool {
        Command::new(self.executable())
            .args([
                "inspect",
                "--type",
                "container",
                "--format",
                "{{.State.Running}}",
                container_id,
            ])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .is_some_and(|output| String::from_utf8_lossy(&output.stdout).trim() == "true")
    }

    /// Stop one container by immutable ID.
    pub fn stop_container(self, container_id: &str) -> Result<()> {
        let output = Command::new(self.executable())
            .args(["stop", container_id])
            .output()
            .map_err(|error| {
                VmError::general(error, format!("Failed to stop container {container_id}"))
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_error("stop container", &output.stderr))
        }
    }
}

fn command_error(operation: &str, stderr: &[u8]) -> VmError {
    VmError::Provider(format!(
        "Failed to {operation}: {}",
        String::from_utf8_lossy(stderr).trim()
    ))
}

fn parse_container_network(output: &str, container_name: &str) -> Result<(String, String)> {
    output
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let network = parts.next()?;
            let address = parts.next()?;
            Some((network.to_string(), address.to_string()))
        })
        .ok_or_else(|| {
            VmError::Provider(format!(
                "No network with an IP address found for {container_name}"
            ))
        })
}

impl ComposeRuntime {
    pub(crate) fn command(
        self,
        executable: &str,
        compose_path: &Path,
        subcommand: &str,
        extra_args: &[&str],
    ) -> Result<ComposeInvocation> {
        let compose_path = compose_path.to_str().ok_or_else(|| {
            VmError::Internal(format!(
                "Path contains invalid UTF-8: {}",
                compose_path.display()
            ))
        })?;
        let (program, mut args) = match self {
            Self::BuiltIn => (executable.to_string(), vec!["compose".to_string()]),
            Self::Standalone => ("podman-compose".to_string(), Vec::new()),
        };
        args.extend([
            "-f".to_string(),
            compose_path.to_string(),
            subcommand.to_string(),
        ]);
        args.extend(extra_args.iter().map(|argument| (*argument).to_string()));
        Ok(ComposeInvocation { program, args })
    }
}

impl ComposeInvocation {
    pub(crate) fn extend<'a>(&mut self, args: impl IntoIterator<Item = &'a str>) {
        self.args.extend(args.into_iter().map(str::to_string));
    }

    pub(crate) fn stream(&self) -> Result<()> {
        let args = self.args.iter().map(String::as_str).collect::<Vec<_>>();
        stream_command(&self.program, &args)
    }
}

fn detect_podman_compose() -> Result<PodmanCompose> {
    if Command::new("podman")
        .args(["compose", "version"])
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(PodmanCompose::BuiltIn);
    }
    if Command::new("podman-compose")
        .arg("version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        return Ok(PodmanCompose::Standalone);
    }
    Err(VmError::Dependency(
        "Neither 'podman compose' nor 'podman-compose' is available".to_string(),
    ))
}

fn install_error(engine: ContainerEngine) -> VmError {
    match engine {
        ContainerEngine::Docker => VmError::DockerNotInstalled(
            "Install from: https://docs.docker.com/get-docker/".to_string(),
        ),
        ContainerEngine::Podman(_) => VmError::Dependency(
            "Podman is not installed. Install from: https://podman.io/docs/installation"
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_container_network, ComposeRuntime, PodmanCompose};
    use std::path::Path;

    #[test]
    fn compose_invocation_encodes_builtin_and_standalone_forms() {
        let builtin = ComposeRuntime::BuiltIn
            .command("podman", Path::new("compose.yml"), "up", &["-d"])
            .unwrap();
        assert_eq!(builtin.program, "podman");
        assert_eq!(builtin.args, ["compose", "-f", "compose.yml", "up", "-d"]);

        let standalone = ComposeRuntime::Standalone
            .command("podman", Path::new("compose.yml"), "up", &["-d"])
            .unwrap();
        assert_eq!(standalone.program, "podman-compose");
        assert_eq!(standalone.args, ["-f", "compose.yml", "up", "-d"]);
        assert_ne!(PodmanCompose::BuiltIn, PodmanCompose::Standalone);
    }

    #[test]
    fn container_network_uses_the_first_addressed_network() {
        assert_eq!(
            parse_container_network("project_default 172.20.0.3\nother 10.0.0.2\n", "demo")
                .unwrap(),
            ("project_default".into(), "172.20.0.3".into())
        );
        assert!(parse_container_network("", "demo").is_err());
    }
}
