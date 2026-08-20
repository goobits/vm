use serde::{Deserialize, Serialize};
use vm_config::config::ProviderName;
use vm_provider::container::ContainerEngine;

use crate::error::{VmError, VmResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ApplianceState {
    #[serde(default)]
    pub definition_revision: u32,
    /// Kept as `runtime` on disk so existing Docker state remains readable.
    #[serde(rename = "runtime")]
    pub engine: ProviderName,
    pub gateway_url: String,
    pub gateway_port: u16,
    pub registry_image: String,
    #[serde(default)]
    pub registry_image_identity: String,
    #[serde(default, alias = "review_image")]
    pub job_image: String,
    pub controller_version: String,
}

impl ApplianceState {
    pub(super) fn container_engine(&self) -> VmResult<ContainerEngine> {
        if !self.engine.is_container() {
            return Err(VmError::validation(
                format!("Invalid package appliance engine '{}'", self.engine),
                Some("Run `vm packages up --engine docker|podman`"),
            ));
        }
        ContainerEngine::detect(&self.engine).map_err(VmError::from)
    }

    pub(super) fn to_json(&self) -> VmResult<Vec<u8>> {
        let mut json = serde_json::to_vec_pretty(self)?;
        json.push(b'\n');
        Ok(json)
    }

    pub(super) fn from_json(json: &[u8]) -> VmResult<Self> {
        serde_json::from_slice(json).map_err(VmError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::ApplianceState;
    use vm_config::config::ProviderName;

    #[test]
    fn state_round_trips_without_changing_the_docker_disk_shape() {
        let state = ApplianceState {
            definition_revision: 3,
            engine: ProviderName::Docker,
            gateway_url: "http://127.0.0.1:3080".into(),
            gateway_port: 3080,
            registry_image: "registry.example/vm-packages:1".into(),
            registry_image_identity: "sha256:registry-image".into(),
            job_image: "registry.example/vm-package-jobs:1".into(),
            controller_version: "1.0.0".into(),
        };
        let json = state.to_json().unwrap();
        assert!(String::from_utf8_lossy(&json).contains("\"runtime\": \"docker\""));
        assert_eq!(ApplianceState::from_json(&json).unwrap(), state);
    }

    #[test]
    fn retired_tart_appliance_state_is_rejected() {
        let state = ApplianceState::from_json(
            br#"{
                "runtime": "tart",
                "gateway_url": "http://192.0.2.2:3080",
                "gateway_port": 3080,
                "registry_image": "registry:1",
                "job_image": "jobs:1",
                "controller_version": "1"
            }"#,
        )
        .unwrap();
        assert!(state.container_engine().is_err());
    }
}
