use std::path::Path;
use std::process::Command;

use vm_config::config::ProviderName;
use vm_core::command_stream::{is_tool_installed, stream_command, stream_command_visible};
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

    pub(crate) fn stream_visible(&self) -> Result<()> {
        let args = self.args.iter().map(String::as_str).collect::<Vec<_>>();
        stream_command_visible(&self.program, &args)
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
    use super::{ComposeRuntime, PodmanCompose};
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
}
