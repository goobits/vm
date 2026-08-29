use std::path::{Path, PathBuf};

use vm_config::config::{PackageEdgeConfig, VmConfig};
use vm_packages::{
    AgentCapabilityClaims, ClientEnvironment, RegistryEndpoints, ToolSourceAttestation,
};

use crate::error::{VmError, VmResult};

use super::{appliance, files::ApplianceFiles, state::ApplianceState};

// Bump when edge labels, environment, mounts, or lifecycle policy change
// without requiring a new registry image.
const PACKAGE_EDGE_POLICY_REVISION: &str = "3";

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
    let global = vm_config::GlobalConfig::load()?;
    let canonical_source = canonical_source(config, &global)?;
    let (client, edge) = client_environment(
        &state,
        files.read_token()?,
        &files.agent_signing_key()?,
        consumer,
        canonical_source.as_ref(),
        provider,
        workspace_path(config),
    )?;
    Ok(Some((client, edge)))
}

#[derive(Debug, Clone)]
struct CanonicalSource {
    repository: String,
    tool: Option<ToolSourceAttestation>,
}

fn canonical_source(
    config: &VmConfig,
    global: &vm_config::GlobalConfig,
) -> VmResult<Option<CanonicalSource>> {
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
    let repository = vm_packages::normalize_remote_repository_url(&repository.origin_url)
        .map_err(VmError::from)?;
    let tool = if project.join("vm-tool.yaml").is_file() {
        let request = super::discovery::discover_tool(&project, None, true)?;
        global
            .tools
            .contains_key(&request.name)
            .then(|| ToolSourceAttestation::new(request).map_err(VmError::from))
            .transpose()?
    } else {
        None
    };
    Ok(Some(CanonicalSource { repository, tool }))
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
        .filter_map(|source| Path::new(source).canonicalize().ok())
        .any(|source| source == project)
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
    canonical_source: Option<&CanonicalSource>,
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
        "docker" | "podman" => format!("http://{consumer}-package-edge:3080"),
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
    let mut claims = AgentCapabilityClaims::new(
        consumer,
        canonical_source.map(|source| source.repository.clone()),
    )
    .map_err(VmError::from)?;
    if let Some(tool) = canonical_source.and_then(|source| source.tool.clone()) {
        claims = claims.with_tool_source(tool).map_err(VmError::from)?;
    }
    let agent_token = if canonical_source.is_some() {
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
    match provider {
        "docker" | "podman" => Ok(format!(
            "http://{}:{}",
            vm_platform::platform::get_host_gateway(),
            state.gateway_port
        )),
        // Tart's Virtualization.framework network exposes the controller host
        // at the first address on its private subnet.
        "tart" => Ok(format!("http://192.168.64.1:{}", state.gateway_port)),
        _ => Err(VmError::validation(
            format!("Provider '{provider}' cannot reach the package appliance"),
            Some("Use Docker, Podman, or a Linux Tart guest"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ApplianceState {
        ApplianceState {
            definition_revision: vm_packages::APPLIANCE_DEFINITION_REVISION,
            engine: vm_config::config::ProviderName::Docker,
            gateway_url: "http://127.0.0.1:3080".into(),
            gateway_port: 3080,
            registry_image: "registry/image:1".into(),
            registry_image_identity: "sha256:image-1".into(),
            job_image: "jobs/image:1".into(),
            controller_version: "1".into(),
        }
    }

    #[test]
    fn one_container_appliance_serves_docker_and_tart_guests() {
        let state = state();
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
        assert!(docker_variables["NPM_CONFIG_REGISTRY"].contains("project-a-package-edge"));
        assert!(tart_variables["NPM_CONFIG_REGISTRY"].contains("127.0.0.1"));
        assert!(docker_variables["VM_OCI_MIRROR"].ends_with(":3080"));
        assert_eq!(docker_variables["VM_PACKAGES_CONSUMER"], "project-a");
        assert_eq!(tart_variables["VM_OCI_MIRROR"], "http://192.168.64.1:3080");
        assert_eq!(
            docker_edge.client_gateway,
            "http://project-a-package-edge:3080"
        );
        assert_eq!(tart_edge.client_gateway, "http://127.0.0.1:3080");
        assert!(docker_edge.internal_gateway.ends_with(":3080"));
        assert_eq!(tart_edge.internal_gateway, "http://192.168.64.1:3080");
    }

    #[test]
    fn canonical_repository_issues_a_v2_workspace_capability() {
        let signing_key = "agent-signing-key-012345678901234567890123456789";
        let (client, _) = client_environment(
            &state(),
            "read-token".into(),
            signing_key,
            "project-a",
            Some(&CanonicalSource {
                repository: "https://github.com/team/project.git".into(),
                tool: None,
            }),
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

    #[cfg(unix)]
    #[test]
    fn canonical_authority_resolves_registered_filesystem_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let registered = directory.path().join("registered");
        let alias = directory.path().join("alias");
        std::fs::create_dir_all(&registered).unwrap();
        std::os::unix::fs::symlink(&registered, &alias).unwrap();
        let config_path = registered.join("vm.yaml");
        std::fs::write(&config_path, "version: '2.0'\n").unwrap();
        let config = VmConfig {
            source_path: Some(config_path),
            ..Default::default()
        };

        assert!(
            canonical_project_root(&config, &[alias.to_string_lossy().into_owned()])
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn edge_revision_tracks_controller_and_runtime_policy() {
        let mut first = state();
        let initial = package_edge_revision(&first, &first.gateway_url, "read-token");
        first.controller_version = "2".into();
        let upgraded = package_edge_revision(&first, &first.gateway_url, "read-token");
        assert_ne!(initial, upgraded);
        first.registry_image_identity = "sha256:image-2".into();
        let rebuilt = package_edge_revision(&first, &first.gateway_url, "read-token");
        assert_ne!(upgraded, rebuilt);
    }

    #[test]
    fn tart_guests_use_the_controller_side_of_the_vmnet_subnet() {
        assert_eq!(
            gateway_for_provider(&state(), "tart").unwrap(),
            "http://192.168.64.1:3080"
        );
    }
}
