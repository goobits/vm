use anyhow::{bail, Result};

/// Stable gateway endpoints exposed to a project environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEndpoints {
    gateway: String,
}

impl RegistryEndpoints {
    pub fn new(gateway: impl Into<String>) -> Result<Self> {
        let gateway = gateway.into().trim_end_matches('/').to_string();
        if gateway.is_empty() {
            bail!("package gateway URL cannot be empty");
        }
        Ok(Self { gateway })
    }

    pub fn gateway(&self) -> &str {
        &self.gateway
    }

    pub fn npm(&self) -> String {
        format!("{}/npm/", self.gateway)
    }

    pub fn pypi(&self) -> String {
        format!("{}/pypi/simple/", self.gateway)
    }

    pub fn cargo_index(&self) -> String {
        format!("sparse+{}/cargo/index/", self.gateway)
    }

    pub fn api(&self) -> String {
        format!("{}/api", self.gateway)
    }
}

/// Provider-neutral environment injected into Docker and Tart guests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientEnvironment {
    endpoints: RegistryEndpoints,
}

impl ClientEnvironment {
    pub fn new(endpoints: RegistryEndpoints) -> Self {
        Self { endpoints }
    }

    pub fn variables(&self) -> Vec<(String, String)> {
        vec![
            ("NPM_CONFIG_REGISTRY".into(), self.endpoints.npm()),
            ("PIP_INDEX_URL".into(), self.endpoints.pypi()),
            (
                "VM_CARGO_REGISTRY_INDEX".into(),
                self.endpoints.cargo_index(),
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientEnvironment, RegistryEndpoints};

    #[test]
    fn creates_protocol_urls_from_one_gateway() {
        let endpoints = RegistryEndpoints::new("https://packages.internal/").unwrap();
        assert_eq!(endpoints.npm(), "https://packages.internal/npm/");
        assert_eq!(
            endpoints.cargo_index(),
            "sparse+https://packages.internal/cargo/index/"
        );
        assert_eq!(ClientEnvironment::new(endpoints).variables().len(), 3);
    }
}
