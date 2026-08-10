use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const COMPOSE_PROJECT: &str = "vm-packages";
pub const TART_INSTANCE_NAME: &str = "vm-packages-infra";
pub const TART_BASE_NAME: &str = "vibe-tart-linux-base";
pub const COMPOSE_YAML: &str = include_str!("resources/compose.yaml");
pub const GATEWAY_CONFIG: &str = include_str!("resources/Caddyfile");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfrastructureRuntime {
    Docker,
    Tart,
}

impl InfrastructureRuntime {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Tart => "tart",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplianceState {
    pub runtime: InfrastructureRuntime,
    pub gateway_url: String,
    pub gateway_port: u16,
    pub registry_image: String,
    pub controller_version: String,
}

impl ApplianceState {
    pub fn to_json(&self) -> Result<Vec<u8>> {
        let mut json = serde_json::to_vec_pretty(self)?;
        json.push(b'\n');
        Ok(json)
    }

    pub fn from_json(json: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(json)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplianceConfig {
    pub bind_address: String,
    pub gateway_port: u16,
    pub registry_image: String,
}

impl ApplianceConfig {
    pub fn new(
        bind_address: impl Into<String>,
        gateway_port: u16,
        registry_image: impl Into<String>,
    ) -> Result<Self> {
        let bind_address = bind_address.into();
        if !matches!(bind_address.as_str(), "127.0.0.1" | "0.0.0.0") {
            bail!("package gateway bind address must be 127.0.0.1 or 0.0.0.0");
        }
        if gateway_port < 1024 {
            bail!("package gateway port must be between 1024 and 65535");
        }

        let registry_image = checked_image(registry_image.into())?;
        if registry_image.trim().is_empty() {
            bail!("package registry image cannot be empty");
        }

        Ok(Self {
            bind_address,
            gateway_port,
            registry_image,
        })
    }

    pub fn environment(&self) -> String {
        format!(
            "VM_PACKAGES_BIND={}\nVM_PACKAGES_PORT={}\nVM_PACKAGES_REGISTRY_IMAGE={}\n",
            self.bind_address, self.gateway_port, self.registry_image
        )
    }
}

fn checked_image(value: String) -> Result<String> {
    if !value.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '/' | '_' | '-' | ':' | '@')
    }) {
        bail!("registry image contains an unsupported character");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{
        ApplianceConfig, ApplianceState, InfrastructureRuntime, COMPOSE_YAML, GATEWAY_CONFIG,
    };

    #[test]
    fn compose_keeps_private_data_in_named_volumes() {
        let definition: serde_yaml_ng::Value = serde_yaml_ng::from_str(COMPOSE_YAML).unwrap();
        assert!(definition.get("services").is_some());
        assert!(COMPOSE_YAML.contains("registry-npm-artifacts:/data/npm"));
        assert!(COMPOSE_YAML.contains("registry-cargo-artifacts:/data/cargo"));
        assert!(COMPOSE_YAML.contains("registry-pypi-artifacts:/data/pypi"));
        assert!(COMPOSE_YAML.contains("workflow-state:/data/state"));
        assert!(COMPOSE_YAML.contains("workflow-receipts:/data/receipts"));
        assert!(COMPOSE_YAML.contains("agent-temporary-data:/data/agents"));
        assert!(COMPOSE_YAML.contains("source-mirrors:/data/sources"));
        assert!(COMPOSE_YAML.contains("work_controller_token"));
        assert!(GATEWAY_CONFIG.contains("reverse_proxy work:3091"));
        assert!(!COMPOSE_YAML.contains("/var/run/docker.sock"));
        assert!(!COMPOSE_YAML.contains("/workspace"));
        assert!(!COMPOSE_YAML.contains("${HOME}"));
    }

    #[test]
    fn environment_rejects_line_injection() {
        assert!(ApplianceConfig::new("127.0.0.1", 3080, "image\nBAD=value").is_err());
    }

    #[test]
    fn state_round_trips() {
        let state = ApplianceState {
            runtime: InfrastructureRuntime::Tart,
            gateway_url: "http://192.0.2.2:3080".into(),
            gateway_port: 3080,
            registry_image: "registry.example/vm-packages:1".into(),
            controller_version: "1.0.0".into(),
        };
        assert_eq!(
            ApplianceState::from_json(&state.to_json().unwrap()).unwrap(),
            state
        );
    }
}
