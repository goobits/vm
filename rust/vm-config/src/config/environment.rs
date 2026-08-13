use std::path::PathBuf;

use serde::{Deserialize, Deserializer, Serialize};

use super::{DiskLimit, MountAccess};

/// Runtime-only settings for the read-only package proxy beside a worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackageEdgeConfig {
    pub image: String,
    pub internal_gateway: String,
    pub client_gateway: String,
    pub read_token: String,
    pub revision: String,
}

#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MockProviderConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instances: Vec<MockVmInstanceConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_report: Option<VmStatusReportConfig>,
}

#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MockVmInstanceConfig {
    pub name: String,
    pub status: String,
    pub ip_address: Option<String>,
    pub memory_gb: u32,
    pub cpus: u32,
}

#[cfg(feature = "test-helpers")]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmStatusReportConfig {
    pub name: String,
    pub is_running: bool,
    pub ip_address: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<(String, String)>,
}

/// Project identity and workspace settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    #[serde(default, skip_serializing_if = "MountAccess::is_read_write")]
    pub workspace_access: MountAccess,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_template_path: Option<String>,
}

/// Configuration for individual services and databases.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_option_string_or_number"
    )]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buildx: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub driver: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_microphone: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_on_destroy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_git_branch: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_timestamp: Option<bool>,
}

/// Tart virtualization provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TartConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guest_os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_size: Option<DiskLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rosetta: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nested: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_docker: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    #[serde(default)]
    pub enable_debugging: bool,
    #[serde(default = "default_true")]
    pub no_new_privileges: bool,
    #[serde(default)]
    pub user_namespaces: bool,
    #[serde(default)]
    pub read_only_root: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drop_capabilities: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub security_opts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkingConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub networks: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn deserialize_option_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{Error, Visitor};
    use std::fmt;

    struct OptionalVisitor;
    impl<'de> Visitor<'de> for OptionalVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string, number, or null")
        }

        fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E: Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_any(ValueVisitor)
        }
    }

    struct ValueVisitor;
    impl Visitor<'_> for ValueVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or number")
        }

        fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }

        fn visit_i64<E: Error>(self, value: i64) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }

        fn visit_u64<E: Error>(self, value: u64) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }

        fn visit_f64<E: Error>(self, value: f64) -> Result<Self::Value, E> {
            Ok(Some(value.to_string()))
        }
    }

    deserializer.deserialize_option(OptionalVisitor)
}
