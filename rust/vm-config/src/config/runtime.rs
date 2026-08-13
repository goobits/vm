use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::{CpuLimit, MemoryLimit, SwapLimit};

/// Base image, Dockerfile, or snapshot configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum BoxSpec {
    String(String),
    Build {
        dockerfile: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<IndexMap<String, String>>,
    },
}

/// Virtual machine resource and system configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmSettings {
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "box",
        alias = "image"
    )]
    pub r#box: Option<BoxSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpus: Option<CpuLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<SwapLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swappiness: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port_binding: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gui: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_grace_period: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<ContainerLoggingConfig>,
}

impl VmSettings {
    pub fn get_box_spec(&self) -> Option<BoxSpec> {
        self.r#box.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContainerLoggingConfig {
    #[serde(default = "default_logging_driver")]
    pub driver: String,
    #[serde(default = "default_logging_max_size")]
    pub max_size: String,
    #[serde(default = "default_logging_max_files")]
    pub max_files: u32,
}

impl Default for ContainerLoggingConfig {
    fn default() -> Self {
        Self {
            driver: default_logging_driver(),
            max_size: default_logging_max_size(),
            max_files: default_logging_max_files(),
        }
    }
}

fn default_logging_driver() -> String {
    "local".to_string()
}

fn default_logging_max_size() -> String {
    "20m".to_string()
}

fn default_logging_max_files() -> u32 {
    5
}

/// Language runtime and tool version specifications.
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

/// Idempotent project initialization performed by the provisioner.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BootstrapConfig {
    #[serde(default = "default_true")]
    pub dependencies: bool,
    #[serde(default)]
    pub playwright: PlaywrightBootstrapConfig,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            dependencies: true,
            playwright: PlaywrightBootstrapConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct PlaywrightBootstrapConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub browsers: Vec<String>,
}

fn default_true() -> bool {
    true
}
