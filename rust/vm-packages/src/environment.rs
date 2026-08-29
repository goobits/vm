use anyhow::{bail, Result};
use serde::Serialize;
use url::Url;

use crate::{sha256_hex, AgentCapabilityClaims};

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
    canonical_workspace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentAccess {
    gateway: String,
    token: String,
    claims: AgentCapabilityClaims,
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
        if read_token.contains(['\0', '\n', '\r']) {
            bail!("package read token cannot contain control characters");
        }
        Ok(Self {
            oci_mirror: endpoints.gateway().to_string(),
            endpoints,
            read_token,
            agent_access: None,
            canonical_workspace: None,
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
        claims: AgentCapabilityClaims,
    ) -> Result<Self> {
        let gateway = RegistryEndpoints::new(gateway)?.gateway().to_string();
        let token = token.into();
        if token.trim().is_empty() {
            bail!("package agent token cannot be empty");
        }
        let claims = AgentCapabilityClaims::new(claims.consumer, claims.canonical_repository)?;
        self.agent_access = Some(AgentAccess {
            gateway,
            token,
            claims,
        });
        Ok(self)
    }

    pub fn with_canonical_workspace(mut self, workspace: impl Into<String>) -> Result<Self> {
        let workspace = workspace.into();
        if !std::path::Path::new(&workspace).is_absolute() || workspace.contains(['\0', '\n', '\r'])
        {
            bail!("canonical package workspace must be one absolute path");
        }
        self.canonical_workspace = Some(workspace);
        Ok(self)
    }

    pub fn variables(&self) -> Vec<(String, String)> {
        let mut variables = vec![
            ("NPM_CONFIG_REGISTRY".into(), self.endpoints.npm()),
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
            ("NPM_CONFIG_USERCONFIG".into(), "/etc/vm/npmrc".into()),
            ("PIP_CONFIG_FILE".into(), "/etc/vm/pip.conf".into()),
        ];
        if let Some(access) = &self.agent_access {
            variables.extend([
                (
                    "VM_PACKAGES_CLIENT_URL".into(),
                    format!("{}/vm-client", access.gateway),
                ),
                ("VM_PACKAGES_WORK_GATEWAY".into(), access.gateway.clone()),
                ("VM_PACKAGES_AGENT_TOKEN".into(), access.token.clone()),
                (
                    "VM_PACKAGES_CONSUMER".into(),
                    access.claims.consumer.clone(),
                ),
            ]);
        }
        if let Some(workspace) = &self.canonical_workspace {
            variables.push(("VM_PACKAGES_CANONICAL_WORKSPACE".into(), workspace.clone()));
        }
        variables
    }

    pub fn read_token(&self) -> &str {
        &self.read_token
    }

    pub fn managed_settings(&self) -> ManagedClientSettings {
        let npm_registry = self.endpoints.npm();
        let npm_auth_scope = npm_registry
            .strip_prefix("https://")
            .or_else(|| npm_registry.strip_prefix("http://"))
            .expect("registry endpoints are HTTP(S)");
        let pip_index = self.authenticated_url("pypi/simple/");
        let cargo_index = self.endpoints.cargo_index();
        let mut variables = self.variables();
        variables.sort_by(|left, right| left.0.cmp(&right.0));
        let mut profile = std::iter::once(
            "# Managed by VM; changes are replaced during VM reconciliation.\n".to_string(),
        )
        .chain(
            variables
                .into_iter()
                .map(|(name, value)| format!("export {name}={}\n", shell_quote(&value))),
        )
        .collect::<String>();
        profile.push_str(
            r#"if [ -d "$HOME/.cargo/bin" ]; then
  PATH="$HOME/.cargo/bin:$PATH"
fi
vm_node_executable=""
if [ -d "$HOME/.nvm/versions/node" ]; then
  vm_node_executable="$(find "$HOME/.nvm/versions/node" -mindepth 3 -maxdepth 3 -name npm -print 2>/dev/null | sort | tail -n 1)"
fi
if [ -n "$vm_node_executable" ]; then
  PATH="${vm_node_executable%/npm}:$PATH"
fi
export PATH
unset vm_node_executable
"#,
        );
        let npmrc = format!(
            "registry={npm_registry}\nalways-auth=true\n//{npm_auth_scope}:_authToken={}\n",
            self.read_token
        );
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
    use crate::AgentCapabilityClaims;

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
        assert_eq!(variables.len(), 10);
        assert_eq!(variables[0].1, "https://packages.internal/npm/");
        assert!(!variables[0].1.contains("read secret"));

        let agent = environment
            .with_agent_access(
                "https://packages.internal",
                "agent-token",
                AgentCapabilityClaims::new("project-a", None).unwrap(),
            )
            .unwrap()
            .with_canonical_workspace("/workspace")
            .unwrap()
            .variables();
        assert_eq!(agent.len(), 15);
        assert!(agent.contains(&("VM_PACKAGES_CONSUMER".into(), "project-a".into())));
        assert!(agent.contains(&(
            "VM_PACKAGES_CLIENT_URL".into(),
            "https://packages.internal/vm-client".into()
        )));
        assert!(agent.contains(&(
            "VM_PACKAGES_CANONICAL_WORKSPACE".into(),
            "/workspace".into()
        )));
        assert_eq!(variables[3].1, "read secret");
        assert_eq!(variables[7].1, "https://packages.internal");
    }

    #[test]
    fn rejects_non_http_gateways() {
        assert!(RegistryEndpoints::new("file:///tmp/packages").is_err());
        assert!(RegistryEndpoints::new("relative/path").is_err());
        assert!(ClientEnvironment::new(
            RegistryEndpoints::new("https://packages.internal").unwrap(),
            "read-token\nextra=true"
        )
        .is_err());
    }

    #[test]
    fn canonical_workspace_must_be_one_absolute_guest_path() {
        let environment = ClientEnvironment::new(
            RegistryEndpoints::new("https://packages.internal").unwrap(),
            "read-token",
        )
        .unwrap();
        assert!(environment
            .clone()
            .with_canonical_workspace("/workspace")
            .is_ok());
        assert!(environment
            .with_canonical_workspace("../workspace")
            .is_err());
    }

    #[test]
    fn renders_idempotent_native_client_settings() {
        let settings = ClientEnvironment::new(
            RegistryEndpoints::new("https://packages.internal").unwrap(),
            "read-token",
        )
        .unwrap()
        .with_agent_access(
            "https://packages.internal",
            "agent-token",
            AgentCapabilityClaims::new("project-a", None).unwrap(),
        )
        .unwrap()
        .managed_settings();

        assert!(settings.profile.contains("NPM_CONFIG_USERCONFIG"));
        assert!(settings.profile.contains("VM_PACKAGES_AGENT_TOKEN"));
        assert!(settings
            .npmrc
            .contains("//packages.internal/npm/:_authToken=read-token"));
        assert!(settings.pip_conf.contains("/pypi/simple/"));
        assert!(settings.cargo_config.contains("replace-with = \"vm\""));
        assert_eq!(settings.revision.len(), 64);
    }

    #[test]
    fn managed_settings_activate_real_tool_paths() {
        let settings = ClientEnvironment::new(
            RegistryEndpoints::new("https://packages.internal").unwrap(),
            "read secret",
        )
        .unwrap()
        .managed_settings();

        assert!(settings.profile.contains("$HOME/.cargo/bin"));
        assert!(settings.profile.contains("$HOME/.nvm/versions/node"));
    }
}
