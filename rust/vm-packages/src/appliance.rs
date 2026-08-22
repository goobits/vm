use anyhow::{bail, Result};

pub const COMPOSE_PROJECT: &str = "vm-packages";
pub const COMPOSE_YAML: &str = include_str!("resources/compose.yaml");
pub const GATEWAY_CONFIG: &str = include_str!("resources/Caddyfile");
/// Bump when running appliance services must be rebuilt or recreated.
pub const APPLIANCE_DEFINITION_REVISION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplianceConfig {
    pub bind_address: String,
    pub gateway_port: u16,
    pub registry_image: String,
    pub job_image: String,
}

impl ApplianceConfig {
    pub fn new(
        bind_address: impl Into<String>,
        gateway_port: u16,
        registry_image: impl Into<String>,
        job_image: impl Into<String>,
    ) -> Result<Self> {
        let bind_address = bind_address.into();
        if !matches!(bind_address.as_str(), "127.0.0.1" | "0.0.0.0") {
            bail!("package gateway bind address must be 127.0.0.1 or 0.0.0.0");
        }
        if gateway_port < 1024 {
            bail!("package gateway port must be between 1024 and 65535");
        }

        let registry_image = checked_image(registry_image.into())?;
        let job_image = checked_image(job_image.into())?;
        if registry_image.trim().is_empty() {
            bail!("package registry image cannot be empty");
        }
        if job_image.trim().is_empty() {
            bail!("package job image cannot be empty");
        }

        Ok(Self {
            bind_address,
            gateway_port,
            registry_image,
            job_image,
        })
    }

    pub fn environment(&self) -> String {
        format!(
            "VM_PACKAGES_BIND={}\nVM_PACKAGES_PORT={}\nVM_PACKAGES_REGISTRY_IMAGE={}\nVM_PACKAGES_JOB_IMAGE={}\nVM_PACKAGES_VERSION={}\nVM_PACKAGES_DEFINITION_REVISION={}\n",
            self.bind_address,
            self.gateway_port,
            self.registry_image,
            self.job_image,
            env!("CARGO_PKG_VERSION"),
            APPLIANCE_DEFINITION_REVISION
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
    use super::{ApplianceConfig, APPLIANCE_DEFINITION_REVISION, COMPOSE_YAML, GATEWAY_CONFIG};

    #[test]
    fn compose_keeps_private_data_in_named_volumes() {
        let definition: serde_yaml_ng::Value = serde_yaml_ng::from_str(COMPOSE_YAML).unwrap();
        assert!(definition.get("services").is_some());
        let gateway_networks = definition["services"]["gateway"]["networks"]
            .as_sequence()
            .unwrap();
        assert!(gateway_networks.iter().any(|network| network == "packages"));
        assert!(gateway_networks
            .iter()
            .any(|network| network == "controller"));
        assert!(!definition["services"]["registry"]["networks"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|network| network == "controller"));
        assert!(definition["networks"]["packages"]["internal"]
            .as_bool()
            .unwrap());
        assert!(definition["networks"]["controller"]
            .get("internal")
            .is_none());
        assert!(COMPOSE_YAML.contains("registry-metadata:/data"));
        assert!(COMPOSE_YAML.contains("registry-npm-artifacts:/data/npm/tarballs"));
        assert!(COMPOSE_YAML.contains("registry-cargo-artifacts:/data/cargo/crates"));
        assert!(COMPOSE_YAML.contains("registry-pypi-artifacts:/data/pypi/packages"));
        assert!(COMPOSE_YAML.contains("registry-tool-artifacts:/data/tools/artifacts"));
        assert!(COMPOSE_YAML.contains("registry-oci-cache:/var/lib/registry"));
        assert!(COMPOSE_YAML.contains("workflow-state:/data/state"));
        assert!(COMPOSE_YAML.contains("workflow-receipts:/data/receipts"));
        assert!(COMPOSE_YAML.contains("agent-temporary-data:/data/agents"));
        assert!(COMPOSE_YAML.contains("rollout-temporary-data:/data/rollouts"));
        assert!(COMPOSE_YAML.contains("source-mirrors:/data/sources"));
        assert!(definition["services"]["releaser"]["volumes"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|volume| volume == "source-mirrors:/data/sources"));
        assert!(COMPOSE_YAML.contains("package-catalog:/catalog:ro"));
        assert!(COMPOSE_YAML.contains("package-catalog:/data/catalog"));
        assert!(COMPOSE_YAML.contains("infrastructure-backups:/backups"));
        assert!(COMPOSE_YAML.contains("registry-tool-artifacts:/volumes/tools"));
        assert!(COMPOSE_YAML.contains("builder-package-cache:/volumes/builder_cache"));
        assert!(COMPOSE_YAML.contains("volume-init:"));
        assert!(COMPOSE_YAML.contains("cap_add: [\"CHOWN\", \"FOWNER\"]"));
        assert!(COMPOSE_YAML.contains("condition: service_completed_successfully"));
        assert!(COMPOSE_YAML.contains("work_controller_token"));
        assert!(COMPOSE_YAML.contains("publish_token:\n    file: ./publish-token"));
        assert!(COMPOSE_YAML.contains("work_release_token"));
        assert!(COMPOSE_YAML.contains("work_build_token"));
        assert!(COMPOSE_YAML.contains("work_rollout_token"));
        assert!(COMPOSE_YAML.contains("work_agent_signing_key"));
        assert!(definition["services"]["reviewer"].get("volumes").is_none());
        assert!(COMPOSE_YAML.contains("exec pkg-release"));
        assert!(COMPOSE_YAML.contains("exec pkg-build"));
        assert!(COMPOSE_YAML.contains("exec pkg-review"));
        assert!(COMPOSE_YAML.contains("exec pkg-rollout"));
        assert!(COMPOSE_YAML.contains("build-edge:"));
        assert!(COMPOSE_YAML.contains("profiles: [maintenance]"));
        assert!(GATEWAY_CONFIG.contains("dynamic a work 3091"));
        assert!(GATEWAY_CONFIG.contains("dynamic a oci-cache 5000"));
        assert!(GATEWAY_CONFIG.contains("dynamic a registry 3080"));
        assert!(GATEWAY_CONFIG.contains("refresh 2s"));
        assert!(COMPOSE_YAML.contains("VM_PACKAGES_DEFINITION_REVISION"));
        assert!(!COMPOSE_YAML.contains("/var/run/docker.sock"));
        assert!(!COMPOSE_YAML.contains("/workspace"));
        assert!(!COMPOSE_YAML.contains("${HOME}"));

        let builder = &definition["services"]["builder"];
        assert_eq!(builder["user"], "0:0");
        assert_eq!(builder["networks"].as_sequence().unwrap().len(), 1);
        assert_eq!(builder["networks"][0], "packages");
        assert_eq!(builder["volumes"][0], "binary-build-artifacts:/builds");
        assert!(builder.get("secrets").is_none());
        assert!(builder.get("source-mirrors").is_none());
        let builder_text = serde_yaml_ng::to_string(builder).unwrap();
        assert!(builder_text.contains("/run/build-secrets/build-token"));
        assert!(!builder_text.contains("/run/secrets"));
        assert!(!builder_text.contains("publish_token"));
        assert!(!builder_text.contains("release_token"));
        assert!(!builder_text.contains("git_token"));
        let build_edge = &definition["services"]["build-edge"];
        assert_eq!(build_edge["networks"][0], "packages");
        assert_eq!(build_edge["networks"][1], "egress");
        assert!(build_edge.get("ports").is_none());
        assert!(build_edge["environment"]
            .get("PKG_SERVER_READ_TOKEN")
            .is_none());
        assert!(definition["services"]["releaser"]["volumes"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|volume| volume == "binary-build-artifacts:/builds:ro"));
    }

    #[test]
    fn environment_rejects_line_injection() {
        assert!(ApplianceConfig::new("127.0.0.1", 3080, "image\nBAD=value", "review:1").is_err());
    }

    #[test]
    fn environment_versions_the_materialized_definition() {
        let environment = ApplianceConfig::new("127.0.0.1", 3080, "registry:1", "jobs:1")
            .unwrap()
            .environment();
        assert!(environment.contains(&format!(
            "VM_PACKAGES_DEFINITION_REVISION={APPLIANCE_DEFINITION_REVISION}\n"
        )));
    }
}
