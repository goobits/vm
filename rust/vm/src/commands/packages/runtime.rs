use std::io::Write;

use vm_config::config::{PackageEdgeConfig, VmConfig};
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
    if provider == "tart"
        && (config.os.as_deref() == Some("macos")
            || config
                .tart
                .as_ref()
                .and_then(|tart| tart.guest_os.as_deref())
                == Some("macos"))
    {
        return Err(VmError::validation(
            "The managed package edge requires a Linux Tart guest",
            Some("Use the vibe-tart Linux profile"),
        ));
    }
    let (client, edge) = client_environment(&state, files.read_token()?, provider)?;
    config.package_edge = Some(edge);
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
) -> VmResult<(ClientEnvironment, PackageEdgeConfig)> {
    if state.registry_image.trim().is_empty() {
        return Err(VmError::validation(
            "Package appliance state predates worker-edge support",
            Some("Run `vm packages up` to refresh it"),
        ));
    }
    let internal_gateway = gateway_for_provider(state, provider)?;
    let client_gateway = match provider {
        "docker" | "podman" => "http://package-edge:3080".to_string(),
        "tart" => "http://127.0.0.1:3080".to_string(),
        _ => {
            return Err(VmError::validation(
                format!("Provider '{provider}' does not support the managed package edge"),
                Some("Use Docker, Podman, or a Linux Tart guest"),
            ))
        }
    };
    let revision = vm_packages::sha256_hex(format!(
        "{}\0{}\0{}",
        state.registry_image, internal_gateway, read_token
    ));
    let edge = PackageEdgeConfig {
        image: state.registry_image.clone(),
        internal_gateway: internal_gateway.clone(),
        client_gateway: client_gateway.clone(),
        read_token: read_token.clone(),
        revision,
    };
    let client = ClientEnvironment::new(
        RegistryEndpoints::new(client_gateway).map_err(VmError::from)?,
        read_token,
    )
    .and_then(|client| client.with_oci_mirror(internal_gateway))
    .map_err(VmError::from)?;
    Ok((client, edge))
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

pub(super) fn checkout_root(subject: &RuntimeSubject, checkout_id: &str) -> VmResult<String> {
    checkout_root_for(&subject.config, subject.provider.name(), checkout_id)
}

fn checkout_root_for(config: &VmConfig, provider: &str, checkout_id: &str) -> VmResult<String> {
    vm_packages::validate_managed_id("checkout ID", checkout_id).map_err(VmError::from)?;
    let user = match provider {
        "tart" => config
            .tart
            .as_ref()
            .and_then(|tart| tart.ssh_user.as_deref())
            .unwrap_or("admin"),
        _ => config
            .vm
            .as_ref()
            .and_then(|vm| vm.user.as_deref())
            .unwrap_or("developer"),
    };
    vm_packages::validate_label("guest user", user).map_err(VmError::from)?;
    let home = if user == "root" {
        "/root".to_string()
    } else if provider == "tart"
        && (config.os.as_deref() == Some("macos")
            || config
                .tart
                .as_ref()
                .and_then(|tart| tart.guest_os.as_deref())
                == Some("macos"))
    {
        format!("/Users/{user}")
    } else {
        format!("/home/{user}")
    };
    Ok(format!(
        "{home}/.local/share/vm/package-checkouts/{checkout_id}"
    ))
}

pub(super) fn exec<I, S>(subject: &RuntimeSubject, command: I) -> VmResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let command = command.into_iter().map(Into::into).collect::<Vec<_>>();
    subject
        .provider
        .exec(Some(subject.target.as_str()), &command)
        .map_err(VmError::from)
}

pub(super) fn exec_output<I, S>(subject: &RuntimeSubject, command: I) -> VmResult<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let command = command.into_iter().map(Into::into).collect::<Vec<_>>();
    subject
        .provider
        .exec_output(Some(subject.target.as_str()), &command)
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

pub(super) fn exec_in_workspace<I, S>(subject: &RuntimeSubject, command: I) -> VmResult<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut wrapped = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        "cd \"$1\"; shift; exec \"$@\"".to_string(),
        "vm-package-workspace".to_string(),
        workspace_path(&subject.config).to_string(),
    ];
    wrapped.extend(command.into_iter().map(Into::into));
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
    use super::{apply_environment, checkout_root_for, client_environment, gateway_for_provider};
    use vm_config::config::{TartConfig, VmConfig, VmSettings};
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
        let (docker, docker_edge) =
            client_environment(&state, "read-token".into(), "docker").unwrap();
        let (tart, tart_edge) = client_environment(&state, "read-token".into(), "tart").unwrap();
        let docker_variables = docker
            .variables()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        let tart_variables = tart
            .variables()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        assert!(docker_variables["NPM_CONFIG_REGISTRY"].contains("package-edge"));
        assert!(tart_variables["NPM_CONFIG_REGISTRY"].contains("127.0.0.1"));
        assert_eq!(docker_variables["VM_OCI_MIRROR"], state.gateway_url);
        assert_eq!(tart_variables["VM_OCI_MIRROR"], state.gateway_url);
        assert_eq!(docker_edge.client_gateway, "http://package-edge:3080");
        assert_eq!(tart_edge.client_gateway, "http://127.0.0.1:3080");
        assert_eq!(docker_edge.internal_gateway, state.gateway_url);
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

    #[test]
    fn checkout_roots_cannot_escape_guest_temporary_storage() {
        let docker = VmConfig {
            vm: Some(VmSettings {
                user: Some("developer".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            checkout_root_for(&docker, "docker", "pkg-auth-20260811-000001").unwrap(),
            "/home/developer/.local/share/vm/package-checkouts/pkg-auth-20260811-000001"
        );
        for invalid in ["../workspace", "/workspace", "scope/auth", "."] {
            assert!(checkout_root_for(&docker, "docker", invalid).is_err());
        }

        let tart = VmConfig {
            tart: Some(TartConfig {
                ssh_user: Some("admin".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            checkout_root_for(&tart, "tart", "pkg-auth-20260811-000001").unwrap(),
            "/home/admin/.local/share/vm/package-checkouts/pkg-auth-20260811-000001"
        );
    }
}
