use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use vm_packages::RegistryEndpoints;

use crate::error::{VmError, VmResult};

use super::guest_checkout::infer_checkout_id;

pub(super) struct GuestRuntime {
    consumer: String,
    gateway: String,
    agent_token: String,
    workspace: String,
    canonical_workspace: Option<PathBuf>,
    home: PathBuf,
}

impl GuestRuntime {
    pub(super) fn discover() -> VmResult<Self> {
        let consumer = required_guest_variable("VM_PACKAGES_CONSUMER")?;
        vm_packages::validate_label("consumer", &consumer).map_err(VmError::from)?;
        let gateway = required_guest_variable("VM_PACKAGES_WORK_GATEWAY")?;
        RegistryEndpoints::new(&gateway).map_err(VmError::from)?;
        let agent_token = required_guest_variable("VM_PACKAGES_AGENT_TOKEN")?;
        let canonical_workspace = std::env::var("VM_PACKAGES_CANONICAL_WORKSPACE")
            .ok()
            .filter(|workspace| !workspace.trim().is_empty())
            .map(PathBuf::from);
        let current_dir = std::env::current_dir().map_err(VmError::from)?;
        let workspace = effective_workspace(&current_dir, canonical_workspace.as_deref())
            .to_string_lossy()
            .into_owned();
        let home = dirs::home_dir().ok_or_else(|| {
            VmError::validation("Guest home directory is unavailable", None::<String>)
        })?;
        Ok(Self {
            consumer,
            gateway,
            agent_token,
            workspace,
            canonical_workspace,
            home,
        })
    }

    pub(super) fn consumer(&self) -> &str {
        &self.consumer
    }

    pub(super) fn agent_token(&self) -> &str {
        &self.agent_token
    }

    pub(super) fn home(&self) -> &Path {
        &self.home
    }

    pub(super) fn workspace(&self) -> &str {
        &self.workspace
    }

    pub(super) fn canonical_workspace(&self) -> VmResult<&Path> {
        self.canonical_workspace.as_deref().ok_or_else(|| {
            VmError::validation(
                "Managed guest package access has no canonical workspace binding",
                Some("Run `vm tools update` on the controller host, then open a new guest shell"),
            )
        })
    }

    pub(super) fn request_state_path(&self, key: &str) -> VmResult<PathBuf> {
        vm_packages::validate_managed_id("request key", key).map_err(VmError::from)?;
        Ok(self
            .home
            .join(".local/state/vm/package-requests")
            .join(format!("{key}.json")))
    }

    pub(super) fn current_checkout_id(&self) -> VmResult<Option<String>> {
        infer_checkout_id(&std::env::current_dir().map_err(VmError::from)?, &self.home)
    }

    pub(super) fn client(&self) -> VmResult<vm_packages::PackageInfrastructureClient> {
        Ok(vm_packages::PackageInfrastructureClient::new(
            RegistryEndpoints::new(&self.gateway).map_err(VmError::from)?,
        )
        .with_agent_token(&self.agent_token))
    }

    fn run(&self, command: &[String]) -> VmResult<()> {
        let (program, arguments) = command.split_first().ok_or_else(|| {
            VmError::validation("Package command cannot be empty", None::<String>)
        })?;
        let status = Command::new(program)
            .args(arguments)
            .stdin(Stdio::null())
            .status()
            .map_err(VmError::from)?;
        if status.success() {
            Ok(())
        } else {
            Err(VmError::validation(
                format!("Package command failed with {status}: {program}"),
                None::<String>,
            ))
        }
    }

    fn output(&self, command: &[String]) -> VmResult<String> {
        let (program, arguments) = command.split_first().ok_or_else(|| {
            VmError::validation("Package command cannot be empty", None::<String>)
        })?;
        let output = Command::new(program)
            .args(arguments)
            .stdin(Stdio::null())
            .output()
            .map_err(VmError::from)?;
        if !output.status.success() {
            return Err(VmError::validation(
                format!("Package command failed with {}: {program}", output.status),
                None::<String>,
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| VmError::general(error, "Package command returned non-UTF-8 output"))
    }
}

fn effective_workspace<'a>(
    current_dir: &'a Path,
    canonical_workspace: Option<&'a Path>,
) -> &'a Path {
    canonical_workspace.unwrap_or(current_dir)
}

fn required_guest_variable(name: &str) -> VmResult<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            VmError::validation(
                format!("Managed guest package access is missing {name}"),
                Some("Run `vm tools update` on the controller host, then open a new guest shell"),
            )
        })
}

pub(super) fn exec<I, S>(subject: &GuestRuntime, command: I) -> VmResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let command = command.into_iter().map(Into::into).collect::<Vec<_>>();
    subject.run(&command)
}

pub(super) fn exec_output<I, S>(subject: &GuestRuntime, command: I) -> VmResult<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let command = command.into_iter().map(Into::into).collect::<Vec<_>>();
    subject.output(&command)
}

pub(super) fn exec_in_workspace<I, S>(subject: &GuestRuntime, command: I) -> VmResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut wrapped = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "cd \"$1\"; shift; exec \"$@\"".to_string(),
        "vm-package-workspace".to_string(),
        subject.workspace().to_string(),
    ];
    wrapped.extend(command.into_iter().map(Into::into));
    subject.run(&wrapped)
}

#[cfg(test)]
mod tests {
    use super::effective_workspace;
    use std::path::Path;

    #[test]
    fn consumer_commands_stay_in_the_canonical_workspace() {
        let checkout = Path::new("/home/developer/.local/share/vm/package-checkouts/pkg-1/source");
        let workspace = Path::new("/workspace");

        assert_eq!(effective_workspace(checkout, Some(workspace)), workspace);
        assert_eq!(effective_workspace(checkout, None), checkout);
    }
}
