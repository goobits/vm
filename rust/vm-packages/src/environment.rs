use anyhow::{bail, Result};
use url::Url;

/// Stable gateway endpoints exposed to a project environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryEndpoints {
    gateway: Url,
}

impl RegistryEndpoints {
    pub fn new(gateway: impl Into<String>) -> Result<Self> {
        let gateway = gateway.into();
        let mut gateway = Url::parse(gateway.trim())?;
        if !matches!(gateway.scheme(), "http" | "https") || gateway.host_str().is_none() {
            bail!("package gateway must be an absolute HTTP(S) URL");
        }
        let path = gateway.path().trim_end_matches('/').to_string();
        gateway.set_path(&path);
        gateway.set_query(None);
        gateway.set_fragment(None);
        Ok(Self { gateway })
    }

    pub fn gateway(&self) -> &str {
        self.gateway.as_str().trim_end_matches('/')
    }

    pub fn npm(&self) -> String {
        format!("{}/npm/", self.gateway())
    }

    pub fn pypi(&self) -> String {
        format!("{}/pypi/simple/", self.gateway())
    }

    pub fn cargo_index(&self) -> String {
        format!("sparse+{}/cargo/index/", self.gateway())
    }

    pub fn api(&self) -> String {
        format!("{}/api", self.gateway())
    }

    pub fn oci(&self) -> String {
        format!("{}/v2/", self.gateway())
    }
}

/// Provider-neutral environment injected into Docker and Tart guests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientEnvironment {
    endpoints: RegistryEndpoints,
    read_token: String,
    oci_mirror: String,
}

impl ClientEnvironment {
    pub fn new(endpoints: RegistryEndpoints, read_token: impl Into<String>) -> Result<Self> {
        let read_token = read_token.into();
        if read_token.trim().is_empty() {
            bail!("package read token cannot be empty");
        }
        Ok(Self {
            oci_mirror: endpoints.gateway().to_string(),
            endpoints,
            read_token,
        })
    }

    pub fn with_oci_mirror(mut self, gateway: impl Into<String>) -> Result<Self> {
        self.oci_mirror = RegistryEndpoints::new(gateway)?.gateway().to_string();
        Ok(self)
    }

    pub fn variables(&self) -> Vec<(String, String)> {
        vec![
            ("NPM_CONFIG_REGISTRY".into(), self.authenticated_url("npm/")),
            (
                "PIP_INDEX_URL".into(),
                self.authenticated_url("pypi/simple/"),
            ),
            (
                "CARGO_REGISTRIES_VM_INDEX".into(),
                self.endpoints.cargo_index(),
            ),
            ("CARGO_REGISTRIES_VM_TOKEN".into(), self.read_token.clone()),
            ("CARGO_SOURCE_CRATES_IO_REPLACE_WITH".into(), "vm".into()),
            (
                "CARGO_SOURCE_VM_REGISTRY".into(),
                self.endpoints.cargo_index(),
            ),
            (
                "CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS".into(),
                "cargo:token".into(),
            ),
            ("VM_OCI_MIRROR".into(), self.oci_mirror.clone()),
        ]
    }

    pub fn read_token(&self) -> &str {
        &self.read_token
    }

    fn authenticated_url(&self, path: &str) -> String {
        let mut url = self.endpoints.gateway.clone();
        url.set_path(&format!("{}/{}", url.path().trim_end_matches('/'), path));
        url.set_username("reader")
            .expect("HTTP URLs support usernames");
        url.set_password(Some(&self.read_token))
            .expect("HTTP URLs support passwords");
        url.to_string()
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
        let environment = ClientEnvironment::new(endpoints, "read secret").unwrap();
        let variables = environment.variables();
        assert_eq!(variables.len(), 8);
        assert!(variables[0].1.contains("reader:read%20secret@"));
        assert_eq!(variables[3].1, "read secret");
        assert_eq!(variables[7].1, "https://packages.internal");
    }

    #[test]
    fn rejects_non_http_gateways() {
        assert!(RegistryEndpoints::new("file:///tmp/packages").is_err());
        assert!(RegistryEndpoints::new("relative/path").is_err());
    }
}
