use serde::{Deserialize, Serialize};
use indexmap::IndexMap;
use std::path::PathBuf;

/// Main VM configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct VmConfig {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm: Option<VmSettings>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<VersionsConfig>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub ports: IndexMap<String, u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_range: Option<String>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub services: IndexMap<String, ServiceConfig>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apt_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub npm_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pip_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_packages: Vec<String>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub aliases: IndexMap<String, String>,

    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub environment: IndexMap<String, String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalConfig>,

    #[serde(default, skip_serializing_if = "is_false")]
    pub claude_sync: bool,

    #[serde(default, skip_serializing_if = "is_false")]
    pub gemini_sync: bool,

    #[serde(default, skip_serializing_if = "is_false")]
    pub persist_databases: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_linking: Option<PackageLinkingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup_pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub box_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub swappiness: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_binding: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersionsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub npm: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnpm: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub python: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub nvm: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PackageLinkingConfig {
    #[serde(default = "default_true")]
    pub npm: bool,

    #[serde(default)]
    pub pip: bool,

    #[serde(default)]
    pub cargo: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

fn default_true() -> bool {
    true
}

impl VmConfig {
    /// Load config from YAML file
    pub fn from_file(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(serde_yaml::from_str(&content)?)
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Check if this is a partial config (missing required fields)
    pub fn is_partial(&self) -> bool {
        self.provider.is_none() ||
        self.project.as_ref().map_or(true, |p| p.name.is_none())
    }
}