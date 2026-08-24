use serde::{Deserialize, Serialize};
use vm_config::config::ProviderName;
use vm_provider::container::ContainerEngine;

use crate::error::{VmError, VmResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ApplianceState {
    #[serde(default)]
    pub definition_revision: u32,
    pub engine: ProviderName,
    pub gateway_url: String,
    pub gateway_port: u16,
    pub registry_image: String,
    #[serde(default)]
    pub registry_image_identity: String,
    #[serde(default)]
    pub job_image: String,
    pub controller_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyApplianceState {
    #[serde(default)]
    definition_revision: u32,
    runtime: ProviderName,
    gateway_url: String,
    gateway_port: u16,
    registry_image: String,
    #[serde(default)]
    registry_image_identity: String,
    #[serde(default)]
    job_image: String,
    #[serde(default)]
    review_image: String,
    controller_version: String,
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

    pub(super) fn from_persisted_json(json: &[u8]) -> VmResult<(Self, bool)> {
        match Self::from_json(json) {
            Ok(state) => Ok((state, false)),
            Err(current_error) => {
                let legacy: LegacyApplianceState =
                    serde_json::from_slice(json).map_err(|_| current_error)?;
                let job_image = match (legacy.job_image.as_str(), legacy.review_image.as_str()) {
                    (job_image, "") => job_image.to_string(),
                    ("", review_image) => review_image.to_string(),
                    (job_image, review_image) if job_image == review_image => job_image.to_string(),
                    _ => {
                        return Err(VmError::validation(
                            "Legacy package infrastructure metadata names conflicting job images",
                            Some("Run `vm packages doctor --fix`"),
                        ));
                    }
                };
                Ok((
                    Self {
                        definition_revision: legacy.definition_revision,
                        engine: legacy.runtime,
                        gateway_url: legacy.gateway_url,
                        gateway_port: legacy.gateway_port,
                        registry_image: legacy.registry_image,
                        registry_image_identity: legacy.registry_image_identity,
                        job_image,
                        controller_version: legacy.controller_version,
                    },
                    true,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApplianceState;
    use vm_config::config::ProviderName;

    #[test]
    fn state_round_trips_with_canonical_field_names() {
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
        assert!(String::from_utf8_lossy(&json).contains("\"engine\": \"docker\""));
        assert_eq!(ApplianceState::from_json(&json).unwrap(), state);
    }

    #[test]
    fn retired_state_fields_are_rejected() {
        for retired_field in [r#""runtime": "docker""#, r#""review_image": "jobs:1""#] {
            let json = format!(
                r#"{{
                    "definition_revision": 3,
                    "engine": "docker",
                    "gateway_url": "http://127.0.0.1:3080",
                    "gateway_port": 3080,
                    "registry_image": "registry:1",
                    "registry_image_identity": "sha256:registry",
                    "job_image": "jobs:1",
                    "controller_version": "1",
                    {retired_field}
                }}"#
            );

            assert!(ApplianceState::from_json(json.as_bytes()).is_err());
        }
    }

    #[test]
    fn persisted_runtime_state_migrates_to_the_canonical_engine_field() {
        let json = br#"{
            "definition_revision": 3,
            "runtime": "docker",
            "gateway_url": "http://127.0.0.1:3080",
            "gateway_port": 3080,
            "registry_image": "registry:1",
            "registry_image_identity": "sha256:registry",
            "job_image": "jobs:1",
            "controller_version": "1"
        }"#;

        let (state, migrated) = ApplianceState::from_persisted_json(json).unwrap();

        assert!(migrated);
        assert_eq!(state.engine, ProviderName::Docker);
        assert_eq!(state.job_image, "jobs:1");
        assert!(
            String::from_utf8_lossy(&state.to_json().unwrap()).contains("\"engine\": \"docker\"")
        );
    }

    #[test]
    fn persisted_review_image_state_migrates_once() {
        let json = br#"{
            "runtime": "docker",
            "gateway_url": "http://127.0.0.1:3080",
            "gateway_port": 3080,
            "registry_image": "registry:1",
            "review_image": "jobs:1",
            "controller_version": "1"
        }"#;

        let (state, migrated) = ApplianceState::from_persisted_json(json).unwrap();

        assert!(migrated);
        assert_eq!(state.job_image, "jobs:1");
    }
}
