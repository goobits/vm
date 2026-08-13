use anyhow::{bail, Result};
use serde::Serialize;
use url::Url;

use crate::sha256_hex;

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
    agent_access: Option<AgentAccess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentAccess {
    gateway: String,
    token: String,
    consumer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagedClientSettings {
    pub revision: String,
    pub profile: String,
    pub npmrc: String,
    pub pip_conf: String,
    pub cargo_config: String,
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
            agent_access: None,
        })
    }

    pub fn with_oci_mirror(mut self, gateway: impl Into<String>) -> Result<Self> {
        self.oci_mirror = RegistryEndpoints::new(gateway)?.gateway().to_string();
        Ok(self)
    }

    pub fn with_agent_access(
        mut self,
        gateway: impl Into<String>,
        token: impl Into<String>,
        consumer: impl Into<String>,
    ) -> Result<Self> {
        let gateway = RegistryEndpoints::new(gateway)?.gateway().to_string();
        let token = token.into();
        let consumer = consumer.into();
        if token.trim().is_empty() {
            bail!("package agent token cannot be empty");
        }
        crate::validate_label("consumer", &consumer)?;
        self.agent_access = Some(AgentAccess {
            gateway,
            token,
            consumer,
        });
        Ok(self)
    }

    pub fn variables(&self) -> Vec<(String, String)> {
        let mut variables = vec![
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
        ];
        if let Some(access) = &self.agent_access {
            variables.extend([
                ("VM_PACKAGES_WORK_GATEWAY".into(), access.gateway.clone()),
                ("VM_PACKAGES_AGENT_TOKEN".into(), access.token.clone()),
                ("VM_PACKAGES_CONSUMER".into(), access.consumer.clone()),
            ]);
        }
        variables
    }

    pub fn read_token(&self) -> &str {
        &self.read_token
    }

    pub fn managed_settings(&self) -> ManagedClientSettings {
        let npm_registry = self.authenticated_url("npm/");
        let pip_index = self.authenticated_url("pypi/simple/");
        let cargo_index = self.endpoints.cargo_index();
        let mut variables = self.variables();
        variables.extend([
            ("NPM_CONFIG_USERCONFIG".into(), "/etc/vm/npmrc".into()),
            ("PIP_CONFIG_FILE".into(), "/etc/vm/pip.conf".into()),
        ]);
        variables.sort_by(|left, right| left.0.cmp(&right.0));
        let profile = std::iter::once(
            "# Managed by VM; changes are replaced during VM reconciliation.\n".to_string(),
        )
        .chain(
            variables
                .into_iter()
                .map(|(name, value)| format!("export {name}={}\n", shell_quote(&value))),
        )
        .collect::<String>();
        let npmrc = format!("registry={npm_registry}\nalways-auth=true\n");
        let pip_conf = format!("[global]\nindex-url = {pip_index}\n");
        let cargo_config = format!(
            "[registries.vm]\nindex = {index:?}\ncredential-provider = \"cargo:token\"\n\n[source.crates-io]\nreplace-with = \"vm\"\n\n[source.vm]\nregistry = {index:?}\n\n[registry]\nglobal-credential-providers = [\"cargo:token\"]\n",
            index = cargo_index
        );
        let revision =
            sha256_hex(format!("{profile}\0{npmrc}\0{pip_conf}\0{cargo_config}").as_bytes());
        ManagedClientSettings {
            revision,
            profile,
            npmrc,
            pip_conf,
            cargo_config,
        }
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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
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

        let agent = environment
            .with_agent_access("https://packages.internal", "agent-token", "project-a")
            .unwrap()
            .variables();
        assert_eq!(agent.len(), 11);
        assert!(agent.contains(&("VM_PACKAGES_CONSUMER".into(), "project-a".into())));
        assert_eq!(variables[3].1, "read secret");
        assert_eq!(variables[7].1, "https://packages.internal");
    }

    #[test]
    fn rejects_non_http_gateways() {
        assert!(RegistryEndpoints::new("file:///tmp/packages").is_err());
        assert!(RegistryEndpoints::new("relative/path").is_err());
    }

    #[test]
    fn renders_idempotent_native_client_settings() {
        let settings = ClientEnvironment::new(
            RegistryEndpoints::new("https://packages.internal").unwrap(),
            "read-token",
        )
        .unwrap()
        .with_agent_access("https://packages.internal", "agent-token", "project-a")
        .unwrap()
        .managed_settings();

        assert!(settings.profile.contains("NPM_CONFIG_USERCONFIG"));
        assert!(settings.profile.contains("VM_PACKAGES_AGENT_TOKEN"));
        assert!(settings.npmrc.contains("reader:read-token@"));
        assert!(settings.pip_conf.contains("/pypi/simple/"));
        assert!(settings.cargo_config.contains("replace-with = \"vm\""));
        assert_eq!(settings.revision.len(), 64);
    }
}
