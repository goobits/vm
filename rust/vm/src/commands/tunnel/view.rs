//! Public, redacted tunnel inventory views.

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::state::TunnelInfo;

#[derive(Serialize)]
pub(super) struct TunnelListView<'a> {
    pub project: &'a str,
    pub tunnels: Vec<TunnelView<'a>>,
}

#[derive(Serialize)]
pub(super) struct TunnelView<'a> {
    id: String,
    name: &'a str,
    environment: &'a str,
    provider: &'a str,
    local: LocalEndpoint<'a>,
    remote: RemoteEndpoint<'a>,
    created_at: &'a str,
}

#[derive(Serialize)]
struct LocalEndpoint<'a> {
    address: &'a str,
    port: u16,
}

#[derive(Serialize)]
struct RemoteEndpoint<'a> {
    host: &'a str,
    port: u16,
}

pub(super) fn list<'a>(project: &'a str, records: &'a [TunnelInfo]) -> TunnelListView<'a> {
    TunnelListView {
        project,
        tunnels: records.iter().map(view).collect(),
    }
}

fn view(record: &TunnelInfo) -> TunnelView<'_> {
    let mut digest = Sha256::new();
    for part in [
        record.project_config_path.as_str(),
        record.provider.as_str(),
        record.relay_container_id.as_str(),
    ] {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    let hash = digest.finalize();
    TunnelView {
        id: format!("tunnel-{}", hex_prefix(&hash[..12])),
        name: &record.name,
        environment: &record.container_name,
        provider: &record.provider,
        local: LocalEndpoint {
            address: &record.local_address,
            port: record.host_port,
        },
        remote: RemoteEndpoint {
            host: &record.remote_host,
            port: record.container_port,
        },
        created_at: &record.created_at,
    }
}

fn hex_prefix(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").expect("writing to String cannot fail");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::list;
    use crate::commands::tunnel::state::TunnelInfo;

    #[test]
    fn tunnel_json_omits_owner_path_and_relay_identity() {
        let records = vec![TunnelInfo {
            name: "web".into(),
            project_config_path: "/private/work/vm.yaml".into(),
            provider: "docker".into(),
            local_address: "127.0.0.1".into(),
            remote_host: "api.internal".into(),
            host_port: 41000,
            container_port: 3000,
            container_name: "demo-backend-dev".into(),
            relay_container_id: "secret-container-id".into(),
            relay_container_name: "secret-container-name".into(),
            created_at: "2026-09-25T12:34:56Z".into(),
        }];
        let value = serde_json::to_value(list("demo", &records)).unwrap();
        assert_eq!(value["project"], "demo");
        assert_eq!(value["tunnels"][0]["local"]["port"], 41000);
        assert_eq!(value["tunnels"][0]["remote"]["host"], "api.internal");
        assert!(value["tunnels"][0]["id"]
            .as_str()
            .unwrap()
            .starts_with("tunnel-"));
        let encoded = value.to_string();
        for secret in [
            "/private/work/vm.yaml",
            "secret-container-id",
            "secret-container-name",
        ] {
            assert!(!encoded.contains(secret));
        }
    }
}
