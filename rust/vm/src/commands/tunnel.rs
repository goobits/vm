//! Named TCP tunnels through Docker and Podman environment networks.

use crate::cli::TunnelSubcommand;
use crate::error::{VmError, VmResult};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use vm_config::{config::VmConfig, AppConfig};
use vm_core::{vm_hint, vm_println};
use vm_provider::{Provider, TunnelProvider};

mod state;
mod view;
use state::{ProjectTunnels, TunnelInfo, TunnelManager};

pub(super) fn handle_command(
    command: TunnelSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        TunnelSubcommand::Open {
            name,
            local,
            remote,
            env,
        } => handle_tunnel(&name, &local, &remote, config_path, profile, env),
        TunnelSubcommand::List {
            env,
            provider,
            json,
        } => handle_tunnel_list(config_path, profile, env, provider, json),
        TunnelSubcommand::Close {
            name,
            env,
            provider,
        } => handle_tunnel_stop(&name, config_path, profile, env, provider),
    }
}

/// Handle tunnel command (create a new tunnel)
fn handle_tunnel(
    name: &str,
    local: &str,
    remote: &str,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> VmResult<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(VmError::validation(
            "Tunnel names must contain only letters, digits, hyphens, or underscores",
            None::<String>,
        ));
    }
    let local = parse_endpoint(local, false)?;
    let remote = parse_endpoint(remote, true)?;
    let subject = super::command_context::load_runtime_subject(config_path, profile, environment)?;
    let config_path = subject.config.owning_config_path().ok_or_else(|| {
        VmError::validation("Tunnels require a project configuration", None::<String>)
    })?;

    // Create tunnel
    let manager = TunnelManager::new(
        tunnel_provider(subject.provider.as_ref())?,
        config_path,
        subject.provider.name(),
    )?;
    manager.create_tunnel(
        name,
        &local.host,
        local.port,
        &remote.host,
        remote.port,
        &subject.target,
    )
}

#[derive(Debug)]
struct Endpoint {
    host: String,
    port: u16,
}

fn parse_endpoint(value: &str, remote: bool) -> VmResult<Endpoint> {
    if value.starts_with('[') || value.parse::<std::net::Ipv6Addr>().is_ok() {
        return Err(VmError::validation(
            "IPv6 tunnel endpoints are not supported",
            Some("Use an IPv4 address or DNS name for --remote, and IPv4 for --local"),
        ));
    }
    let (host, port) = value.rsplit_once(':').ok_or_else(|| {
        VmError::validation(
            format!("Invalid endpoint '{value}'"),
            Some("Use HOST:PORT".to_string()),
        )
    })?;
    if host.contains(':') {
        return Err(VmError::validation(
            "IPv6 tunnel endpoints are not supported",
            Some("Use an IPv4 address or DNS name for --remote, and IPv4 for --local"),
        ));
    }
    let host = if host == "localhost" {
        "127.0.0.1"
    } else {
        host
    };
    let valid = if remote {
        !host.is_empty()
            && host.len() <= 253
            && host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            && !host.starts_with('-')
    } else {
        host.parse::<Ipv4Addr>().is_ok()
    };
    if !valid {
        return Err(VmError::validation(
            format!("Invalid endpoint host '{host}'"),
            Some(if remote {
                "Use a DNS name or IPv4 address reachable from the environment".to_string()
            } else {
                "Use localhost or an explicit IPv4 bind address".to_string()
            }),
        ));
    }
    let port: u16 = port.parse().map_err(|_| {
        VmError::validation(
            format!("Invalid port in endpoint '{value}'"),
            None::<String>,
        )
    })?;
    if port == 0 {
        return Err(VmError::validation(
            "Port must be between 1 and 65535",
            None::<String>,
        ));
    }
    Ok(Endpoint {
        host: host.to_string(),
        port,
    })
}

/// Handle tunnel list command
fn handle_tunnel_list(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    provider_filter: Option<String>,
    json: bool,
) -> VmResult<()> {
    let (project, store) = project_tunnels(config_path, profile)?;
    let tunnels = selected_records(
        &store,
        &project,
        environment.as_deref(),
        provider_filter.as_deref(),
    )?;

    if json {
        return crate::presentation::success(
            "tunnels list",
            view::list(super::command_context::project_name(&project), &tunnels),
        );
    }

    if tunnels.is_empty() {
        vm_println!("No recorded tunnels for this project");
        vm_hint!(
            "Create one with: vm tunnels open NAME --local localhost:8080 --remote localhost:3000"
        );
        return Ok(());
    }

    vm_println!("Project tunnels");
    for tunnel in tunnels {
        vm_println!(
            "  {}: {}:{} -> {}:{} via {} ({})",
            tunnel.name,
            tunnel.local_address,
            tunnel.host_port,
            tunnel.remote_host,
            tunnel.container_port,
            tunnel.container_name,
            tunnel.provider
        );
        vm_println!(
            "    Relay: {} | Created: {}",
            tunnel.relay_container_name,
            tunnel.created_at
        );
        vm_println!("");
    }

    Ok(())
}

/// Handle tunnel stop command
fn handle_tunnel_stop(
    name: &str,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    provider_filter: Option<String>,
) -> VmResult<()> {
    let (project, store) = project_tunnels(config_path, profile)?;
    let matches = selected_records(
        &store,
        &project,
        environment.as_deref(),
        provider_filter.as_deref(),
    )?
    .into_iter()
    .filter(|record| record.name == name)
    .collect::<Vec<_>>();
    let selected = single_tunnel(name, &matches)?;
    let provider = super::vm_ops::configured_provider(&project, &selected.provider)?;
    store.close(selected, tunnel_provider(provider.as_ref())?)
}

fn project_tunnels(
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<(VmConfig, ProjectTunnels)> {
    let project = AppConfig::load(config_path, profile, None)?.vm;
    super::command_context::require_project_config(&project)?;
    let config_path = project
        .owning_config_path()
        .expect("project config required");
    let store = ProjectTunnels::new(config_path)?;
    Ok((project, store))
}

fn selected_records(
    store: &ProjectTunnels,
    project: &VmConfig,
    environment: Option<&str>,
    provider: Option<&str>,
) -> VmResult<Vec<TunnelInfo>> {
    Ok(filter_records(
        store.records()?,
        project,
        environment,
        provider,
    ))
}

fn filter_records(
    records: Vec<TunnelInfo>,
    project: &VmConfig,
    environment: Option<&str>,
    provider: Option<&str>,
) -> Vec<TunnelInfo> {
    records
        .into_iter()
        .filter(|record| provider.map_or(true, |name| record.provider == name))
        .filter(|record| {
            environment.map_or(true, |name| environment_matches(record, project, name))
        })
        .collect()
}

fn environment_matches(record: &TunnelInfo, project: &VmConfig, requested: &str) -> bool {
    if record.container_name == requested {
        return true;
    }
    let project_name = super::command_context::project_name(project);
    record.container_name
        == super::vm_ops::target::canonical_instance_name(
            &record.provider,
            project_name,
            Some(requested),
        )
        || (requested == project_name
            && record.container_name
                == super::vm_ops::target::canonical_instance_name(
                    &record.provider,
                    project_name,
                    None,
                ))
}

fn single_tunnel<'a>(name: &str, matches: &'a [TunnelInfo]) -> VmResult<&'a TunnelInfo> {
    match matches {
        [selected] => Ok(selected),
        [] => Err(VmError::validation(
            format!("No recorded tunnel named '{name}'"),
            None::<String>,
        )),
        many => Err(VmError::validation(
            format!(
                "Tunnel '{name}' is ambiguous: {}",
                many.iter()
                    .map(|record| format!(
                        "{}:{} via {}",
                        record.provider, record.container_name, record.host_port
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Some("Select one with --env and --provider"),
        )),
    }
}

fn tunnel_provider(provider: &dyn Provider) -> VmResult<&dyn TunnelProvider> {
    require_tunnel_provider(provider.as_tunnel_provider(), provider.name())
}

fn require_tunnel_provider<'a>(
    capability: Option<&'a dyn TunnelProvider>,
    provider_name: &str,
) -> VmResult<&'a dyn TunnelProvider> {
    capability.ok_or_else(|| {
        VmError::validation(
            format!(
                "Tunnels require a Docker or Podman environment; '{}' is not supported",
                provider_name
            ),
            None::<String>,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::state::TunnelInfo;
    use super::TunnelManager;
    use super::{
        environment_matches, filter_records, parse_endpoint, require_tunnel_provider, single_tunnel,
    };
    use vm_config::config::{ProjectConfig, VmConfig};
    use vm_provider::TunnelProvider;
    use vm_provider::VmResult;

    struct TestRelay;

    impl TunnelProvider for TestRelay {
        fn start_tcp_relay(
            &self,
            relay_name: &str,
            _local_address: &str,
            _host_port: u16,
            _target_instance: &str,
            _remote_host: &str,
            _target_port: u16,
        ) -> VmResult<String> {
            Ok(relay_name.to_string())
        }

        fn relay_is_running(&self, _relay_id: &str) -> bool {
            true
        }

        fn stop_relay(&self, _relay_id: &str) -> VmResult<()> {
            Ok(())
        }
    }

    #[test]
    fn tunnel_creation_rejects_port_collisions_across_projects() {
        let directory = tempfile::tempdir().unwrap();
        let state_file = directory.path().join("active.json");
        let relay = TestRelay;
        let first = TunnelManager {
            state_file: state_file.clone(),
            project_config_path: "project-one".into(),
            provider_name: "docker".into(),
            provider: &relay,
        };
        let second = TunnelManager {
            state_file,
            project_config_path: "project-two".into(),
            provider_name: "docker".into(),
            provider: &relay,
        };
        first
            .create_tunnel("web", "127.0.0.1", 41000, "db.internal", 5432, "demo-dev")
            .unwrap();
        assert!(second
            .create_tunnel("web", "127.0.0.1", 41000, "db.internal", 5432, "demo-dev")
            .is_err());
        first
            .create_tunnel("web", "127.0.0.1", 41000, "db.internal", 5432, "demo-dev")
            .unwrap();
        assert!(first
            .create_tunnel("web", "127.0.0.1", 41001, "db.internal", 5432, "demo-dev")
            .is_err());
    }

    #[test]
    fn tunnels_require_an_explicit_provider_capability() {
        let error = match require_tunnel_provider(None, "tart") {
            Ok(_) => panic!("unsupported provider unexpectedly exposed tunnels"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("'tart' is not supported"));
    }

    #[test]
    fn endpoints_accept_explicit_bind_and_routable_remote_host() {
        let local = parse_endpoint("localhost:8080", false).unwrap();
        assert_eq!(local.host, "127.0.0.1");
        assert_eq!(local.port, 8080);
        assert_eq!(
            parse_endpoint("0.0.0.0:8080", false).unwrap().host,
            "0.0.0.0"
        );
        assert_eq!(
            parse_endpoint("db.example:5432", true).unwrap().host,
            "db.example"
        );
        assert_eq!(
            parse_endpoint("192.0.2.10:5432", true).unwrap().host,
            "192.0.2.10"
        );
        assert!(parse_endpoint("localhost:0", false).is_err());
        assert!(parse_endpoint("hostname:8080", false).is_err());
        assert!(parse_endpoint("db.example,listen:5432", true).is_err());
        for endpoint in ["[::1]:8080", "::1:8080", "2001:db8::1:8080"] {
            assert!(parse_endpoint(endpoint, false)
                .unwrap_err()
                .to_string()
                .contains("IPv6 tunnel endpoints are not supported"));
            assert!(parse_endpoint(endpoint, true)
                .unwrap_err()
                .to_string()
                .contains("IPv6 tunnel endpoints are not supported"));
        }
    }

    #[test]
    fn ambiguous_name_requires_environment_or_provider() {
        let record = |provider: &str, port: u16| TunnelInfo {
            name: "web".into(),
            project_config_path: "owner".into(),
            provider: provider.into(),
            local_address: "127.0.0.1".into(),
            remote_host: "web.internal".into(),
            host_port: port,
            container_port: 3000,
            container_name: "demo-backend-dev".into(),
            relay_container_id: format!("relay-{port}"),
            relay_container_name: format!("relay-{port}"),
            created_at: "now".into(),
        };
        let records = vec![record("docker", 41000), record("podman", 41001)];
        assert!(single_tunnel("web", &records)
            .unwrap_err()
            .to_string()
            .contains("ambiguous"));
        assert_eq!(
            single_tunnel("web", &records[..1]).unwrap().provider,
            "docker"
        );
        let project = VmConfig {
            project: Some(ProjectConfig {
                name: Some("demo".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(environment_matches(&records[0], &project, "backend"));
        assert!(environment_matches(
            &records[1],
            &project,
            "demo-backend-dev"
        ));
        assert!(!environment_matches(&records[0], &project, "other"));
        let selected = filter_records(records.clone(), &project, Some("backend"), Some("podman"));
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].provider, "podman");
    }
}
