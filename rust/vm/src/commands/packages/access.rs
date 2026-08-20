use std::path::{Path, PathBuf};

use vm_config::config::{PackageEdgeConfig, VmConfig};
use vm_packages::{ApplianceState, ClientEnvironment, InfrastructureRuntime, RegistryEndpoints};

use crate::error::{VmError, VmResult};

use super::{appliance, files::ApplianceFiles};

// Bump when edge labels, environment, mounts, or lifecycle policy change
// without requiring a new registry image.
const PACKAGE_EDGE_POLICY_REVISION: &str = "2";

pub(super) fn configured_client_environment(
    config: &VmConfig,
) -> VmResult<Option<(ClientEnvironment, PackageEdgeConfig)>> {
    let files = ApplianceFiles::discover()?;
    let Some(state) = files.read_state()? else {
        return Ok(None);
    };
    let state = appliance::repair_client_access(&files, state)?;
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
    let consumer = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let canonical_repository = canonical_repository(config)?;
    let (client, edge) = client_environment(
        &state,
        files.read_token()?,
        &files.agent_signing_key()?,
        consumer,
        canonical_repository.as_deref(),
        provider,
        workspace_path(config),
    )?;
    Ok(Some((client, edge)))
}

fn canonical_repository(config: &VmConfig) -> VmResult<Option<String>> {
    let global = vm_config::GlobalConfig::load()?;
    let Some(project) = canonical_project_root(config, &global.packages.canonical_sources)? else {
        return Ok(None);
    };
    let repository =
        vm_config::detector::git::detect_repository(&project).map_err(VmError::from)?;
    if repository.root != project {
        return Err(VmError::validation(
            format!("{} is not a Git repository root", project.display()),
            Some("Re-register the exact repository with `vm packages register <local-path>`"),
        ));
    }
    vm_packages::normalize_remote_repository_url(&repository.origin_url)
        .map(Some)
        .map_err(VmError::from)
}

fn canonical_project_root(
    config: &VmConfig,
    canonical_sources: &[String],
) -> VmResult<Option<PathBuf>> {
    let Some(config_path) = config.owning_config_path() else {
        return Ok(None);
    };
    let project = config_path
        .parent()
        .ok_or_else(|| VmError::validation("Project configuration has no parent", None::<String>))?
        .canonicalize()
        .map_err(|error| {
            VmError::filesystem(
                error,
                config_path.display().to_string(),
                "resolve physical project root",
            )
        })?;
    Ok(canonical_sources
        .iter()
        .any(|source| Path::new(source) == project)
        .then_some(project))
}

fn workspace_path(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace")
}

fn client_environment(
    state: &ApplianceState,
    read_token: String,
    agent_signing_key: &str,
    consumer: &str,
    canonical_repository: Option<&str>,
    provider: &str,
    canonical_workspace: &str,
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
    let revision = package_edge_revision(state, &internal_gateway, &read_token);
    let edge = PackageEdgeConfig {
        image: state.registry_image.clone(),
        internal_gateway: internal_gateway.clone(),
        client_gateway: client_gateway.clone(),
        read_token: read_token.clone(),
        revision,
    };
    let claims =
        vm_packages::AgentCapabilityClaims::new(consumer, canonical_repository.map(str::to_string))
            .map_err(VmError::from)?;
    let agent_token = if canonical_repository.is_some() {
        vm_packages::issue_agent_capability_v2(agent_signing_key, &claims)
    } else {
        vm_packages::issue_agent_capability(agent_signing_key, consumer)
    }
    .map_err(VmError::from)?;
    let client = ClientEnvironment::new(
        RegistryEndpoints::new(client_gateway).map_err(VmError::from)?,
        read_token,
    )
    .and_then(|client| client.with_oci_mirror(internal_gateway))
    .and_then(|client| client.with_agent_access(&edge.internal_gateway, agent_token, claims))
    .and_then(|client| client.with_canonical_workspace(canonical_workspace))
    .map_err(VmError::from)?;
    Ok((client, edge))
}

fn package_edge_revision(
    state: &ApplianceState,
    internal_gateway: &str,
    read_token: &str,
) -> String {
    vm_packages::sha256_hex(format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}",
        PACKAGE_EDGE_POLICY_REVISION,
        env!("CARGO_PKG_VERSION"),
        state.controller_version,
        state.registry_image,
        state.registry_image_identity,
        internal_gateway,
        read_token
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state(runtime: InfrastructureRuntime) -> ApplianceState {
        ApplianceState {
            definition_revision: vm_packages::APPLIANCE_DEFINITION_REVISION,
            runtime,
            gateway_url: "http://192.0.2.8:3080".into(),
            gateway_port: 3080,
            registry_image: "registry/image:1".into(),
            registry_image_identity: "sha256:image-1".into(),
            job_image: "jobs/image:1".into(),
            controller_version: "1".into(),
            tart_home: None,
        }
    }

    #[test]
    fn tart_appliance_has_one_provider_neutral_client_shape() {
        let state = state(InfrastructureRuntime::Tart);
        let signing_key = "agent-signing-key-012345678901234567890123456789";
        let (docker, docker_edge) = client_environment(
            &state,
            "read-token".into(),
            signing_key,
            "project-a",
            None,
            "docker",
            "/workspace",
        )
        .unwrap();
        let (tart, tart_edge) = client_environment(
            &state,
            "read-token".into(),
            signing_key,
            "project-a",
            None,
            "tart",
            "/workspace",
        )
        .unwrap();
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
        assert_eq!(docker_variables["VM_PACKAGES_CONSUMER"], "project-a");
        assert_eq!(tart_variables["VM_OCI_MIRROR"], state.gateway_url);
        assert_eq!(docker_edge.client_gateway, "http://package-edge:3080");
        assert_eq!(tart_edge.client_gateway, "http://127.0.0.1:3080");
        assert_eq!(docker_edge.internal_gateway, state.gateway_url);
    }

    #[test]
    fn canonical_repository_issues_a_v2_workspace_capability() {
        let signing_key = "agent-signing-key-012345678901234567890123456789";
        let (client, _) = client_environment(
            &state(InfrastructureRuntime::Docker),
            "read-token".into(),
            signing_key,
            "project-a",
            Some("https://github.com/team/project.git"),
            "docker",
            "/workspace",
        )
        .unwrap();
        let variables = client
            .variables()
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        let claims = vm_packages::verify_agent_capability(
            signing_key,
            &variables["VM_PACKAGES_AGENT_TOKEN"],
        )
        .unwrap();

        assert_eq!(claims.consumer, "project-a");
        assert_eq!(
            claims.canonical_repository.as_deref(),
            Some("https://github.com/team/project.git")
        );
    }

    #[test]
    fn canonical_authority_matches_the_physical_project_root_not_its_origin() {
        let directory = tempfile::tempdir().unwrap();
        let registered = directory.path().join("registered");
        let clone = directory.path().join("clone");
        std::fs::create_dir_all(&registered).unwrap();
        std::fs::create_dir_all(&clone).unwrap();
        let registered_config = registered.join("vm.yaml");
        let clone_config = clone.join("vm.yaml");
        std::fs::write(&registered_config, "version: '2.0'\n").unwrap();
        std::fs::write(&clone_config, "version: '2.0'\n").unwrap();
        let canonical = vec![registered
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned()];

        let registered = VmConfig {
            source_path: Some(registered_config),
            ..Default::default()
        };
        let clone = VmConfig {
            source_path: Some(clone_config),
            ..Default::default()
        };
        assert!(canonical_project_root(&registered, &canonical)
            .unwrap()
            .is_some());
        assert!(canonical_project_root(&clone, &canonical)
            .unwrap()
            .is_none());
    }

    #[test]
    fn edge_revision_tracks_controller_and_runtime_policy() {
        let mut first = state(InfrastructureRuntime::Tart);
        let initial = package_edge_revision(&first, &first.gateway_url, "read-token");
        first.controller_version = "2".into();
        let upgraded = package_edge_revision(&first, &first.gateway_url, "read-token");
        assert_ne!(initial, upgraded);
        first.registry_image_identity = "sha256:image-2".into();
        let rebuilt = package_edge_revision(&first, &first.gateway_url, "read-token");
        assert_ne!(upgraded, rebuilt);
    }

    #[test]
    fn tart_guests_reject_a_loopback_only_docker_appliance() {
        assert!(gateway_for_provider(&state(InfrastructureRuntime::Docker), "tart").is_err());
    }
}
