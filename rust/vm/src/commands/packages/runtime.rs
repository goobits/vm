use std::io::Write;

use vm_config::config::VmConfig;
use vm_packages::{ApplianceState, ClientEnvironment, InfrastructureRuntime, RegistryEndpoints};

use crate::commands::command_context::RuntimeSubject;
use crate::error::{VmError, VmResult};

use super::files::ApplianceFiles;

pub(in crate::commands) fn apply_client_environment(config: &mut VmConfig) -> VmResult<()> {
    let files = ApplianceFiles::discover()?;
    let Some(state) = files.read_state()? else {
        return Ok(());
    };
    let provider = config.provider.as_deref().unwrap_or("docker");
    let client = client_environment(&state, files.read_token()?, provider)?;
    apply_environment(config, &client);
    Ok(())
}

fn apply_environment(config: &mut VmConfig, client: &ClientEnvironment) {
    config.environment.extend(client.variables());
}

fn client_environment(
    state: &ApplianceState,
    read_token: String,
    provider: &str,
) -> VmResult<ClientEnvironment> {
    let gateway = gateway_for_provider(state, provider)?;
    let endpoints = RegistryEndpoints::new(gateway).map_err(VmError::from)?;
    ClientEnvironment::new(endpoints, read_token).map_err(VmError::from)
}

pub(super) fn gateway_for_provider(state: &ApplianceState, provider: &str) -> VmResult<String> {
    match (state.runtime, provider) {
        (InfrastructureRuntime::Docker, "tart") => Err(VmError::validation(
            "A Docker-hosted package appliance is not reachable from Tart guests",
            Some("Run `vm packages up --runtime tart` so every environment can reach it"),
        )),
        (InfrastructureRuntime::Docker, "docker" | "podman") => Ok(format!(
            "http://{}:{}",
            vm_platform::platform::get_host_gateway(),
            state.gateway_port
        )),
        (InfrastructureRuntime::Docker, _) => Err(VmError::validation(
            format!("Provider '{provider}' cannot reach a Docker-hosted package appliance"),
            Some("Use the Tart package infrastructure runtime"),
        )),
        (InfrastructureRuntime::Tart, _) => Ok(state.gateway_url.clone()),
    }
}

pub(super) fn checkout_root(checkout_id: &str) -> String {
    format!("/tmp/vm-package-checkouts/{checkout_id}")
}

pub(super) fn exec<const N: usize>(subject: &RuntimeSubject, command: [&str; N]) -> VmResult<()> {
    let command = command.into_iter().map(str::to_string).collect::<Vec<_>>();
    subject
        .provider
        .exec(Some(subject.target.as_str()), &command)
        .map_err(VmError::from)
}

pub(super) fn copy_private(
    subject: &RuntimeSubject,
    content: &[u8],
    destination: &str,
) -> VmResult<()> {
    let mut temporary = tempfile::NamedTempFile::new().map_err(VmError::from)?;
    temporary.write_all(content).map_err(VmError::from)?;
    temporary.flush().map_err(VmError::from)?;
    subject
        .provider
        .copy(
            &temporary.path().to_string_lossy(),
            destination,
            Some(subject.target.as_str()),
        )
        .map_err(VmError::from)?;
    exec(subject, ["chmod", "600", destination])
}

pub(super) fn exec_in_workspace<const N: usize>(
    subject: &RuntimeSubject,
    command: [&str; N],
) -> VmResult<()> {
    let mut wrapped = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "cd \"$1\"; shift; exec \"$@\"".to_string(),
        "vm-package-workspace".to_string(),
        workspace_path(&subject.config).to_string(),
    ];
    wrapped.extend(command.into_iter().map(str::to_string));
    subject
        .provider
        .exec(Some(subject.target.as_str()), &wrapped)
        .map_err(VmError::from)
}

fn workspace_path(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace")
}

#[cfg(test)]
mod tests {
    use super::{apply_environment, client_environment, gateway_for_provider};
    use vm_config::config::VmConfig;
    use vm_packages::{
        ApplianceState, ClientEnvironment, InfrastructureRuntime, RegistryEndpoints,
    };

    fn state(runtime: InfrastructureRuntime) -> ApplianceState {
        ApplianceState {
            runtime,
            gateway_url: "http://192.0.2.8:3080".into(),
            gateway_port: 3080,
            registry_image: "registry/image:1".into(),
            job_image: "jobs/image:1".into(),
            controller_version: "1".into(),
        }
    }

    #[test]
    fn tart_appliance_has_one_provider_neutral_client_shape() {
        let state = state(InfrastructureRuntime::Tart);
        let docker = client_environment(&state, "read-token".into(), "docker").unwrap();
        let tart = client_environment(&state, "read-token".into(), "tart").unwrap();
        assert_eq!(docker.variables(), tart.variables());
    }

    #[test]
    fn tart_guests_reject_a_loopback_only_docker_appliance() {
        assert!(gateway_for_provider(&state(InfrastructureRuntime::Docker), "tart").is_err());
    }

    #[test]
    fn managed_package_settings_replace_project_redirects() {
        let endpoints = RegistryEndpoints::new("http://packages.internal:3080").unwrap();
        let client = ClientEnvironment::new(endpoints, "read-token").unwrap();
        let mut config = VmConfig::default();
        config.environment.insert(
            "NPM_CONFIG_REGISTRY".into(),
            "https://untrusted.invalid".into(),
        );

        apply_environment(&mut config, &client);

        assert!(config.environment["NPM_CONFIG_REGISTRY"].contains("packages.internal"));
        assert_eq!(
            config.environment["CARGO_SOURCE_CRATES_IO_REPLACE_WITH"],
            "vm"
        );
    }
}
