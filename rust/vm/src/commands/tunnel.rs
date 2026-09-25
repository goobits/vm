//! Named TCP tunnels through Docker and Podman environment networks.

use crate::cli::TunnelSubcommand;
use crate::error::{VmError, VmResult};
use std::net::Ipv4Addr;
use std::path::PathBuf;
use vm_core::{vm_hint, vm_println};
use vm_provider::{Provider, TunnelProvider};

mod state;
use state::TunnelManager;

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
        TunnelSubcommand::List { env } => handle_tunnel_list(config_path, profile, env),
        TunnelSubcommand::Close { name, env } => {
            handle_tunnel_stop(&name, config_path, profile, env)
        }
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

struct Endpoint {
    host: String,
    port: u16,
}

fn parse_endpoint(value: &str, remote: bool) -> VmResult<Endpoint> {
    let (host, port) = value.rsplit_once(':').ok_or_else(|| {
        VmError::validation(
            format!("Invalid endpoint '{value}'"),
            Some("Use HOST:PORT".to_string()),
        )
    })?;
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
) -> VmResult<()> {
    let subject = super::command_context::load_runtime_subject(config_path, profile, environment)?;
    let config_path = subject.config.owning_config_path().ok_or_else(|| {
        VmError::validation("Tunnels require a project configuration", None::<String>)
    })?;
    let manager = TunnelManager::new(
        tunnel_provider(subject.provider.as_ref())?,
        config_path,
        subject.provider.name(),
    )?;
    let tunnels = manager.list_tunnels(Some(&subject.target))?;

    if tunnels.is_empty() {
        vm_println!("No active tunnels for environment: {}", subject.target);
        vm_hint!(
            "Create one with: vm tunnels open NAME --local localhost:8080 --remote localhost:3000"
        );
        return Ok(());
    }

    vm_println!("Active tunnels");
    for tunnel in tunnels {
        vm_println!(
            "  {}: {}:{} -> {}:{} via {}",
            tunnel.name,
            tunnel.local_address,
            tunnel.host_port,
            tunnel.remote_host,
            tunnel.container_port,
            tunnel.container_name
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
) -> VmResult<()> {
    let subject = super::command_context::load_runtime_subject(config_path, profile, environment)?;
    let config_path = subject.config.owning_config_path().ok_or_else(|| {
        VmError::validation("Tunnels require a project configuration", None::<String>)
    })?;
    let manager = TunnelManager::new(
        tunnel_provider(subject.provider.as_ref())?,
        config_path,
        subject.provider.name(),
    )?;
    manager.stop_tunnel(name, Some(&subject.target))
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
    use super::TunnelManager;
    use super::{parse_endpoint, require_tunnel_provider};
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
    fn tunnel_state_is_scoped_to_project_provider_and_environment() {
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
        assert_eq!(first.list_tunnels(Some("demo-dev")).unwrap().len(), 1);
        assert!(first.list_tunnels(Some("demo-test")).unwrap().is_empty());
        assert!(second.list_tunnels(None).unwrap().is_empty());
        assert!(second.stop_tunnel("web", None).is_err());
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
        assert!(parse_endpoint("localhost:0", false).is_err());
        assert!(parse_endpoint("hostname:8080", false).is_err());
        assert!(parse_endpoint("db.example,listen:5432", true).is_err());
    }
}
