use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::MemoryLimit;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StorageConfig {
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub volumes: IndexMap<String, VolumeMountConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tmpfs: Vec<TmpfsMountConfig>,
}

impl StorageConfig {
    pub fn is_empty(&self) -> bool {
        self.volumes.is_empty() && self.tmpfs.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeMountConfig {
    pub target: String,
    #[serde(default)]
    pub scope: VolumeScope,
    #[serde(default = "default_true")]
    pub nocopy: bool,
    #[serde(default)]
    pub retention: VolumeRetention,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VolumeScope {
    #[default]
    Project,
    Instance,
    Platform,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VolumeRetention {
    #[default]
    Keep,
    Disposable,
}

impl VolumeRetention {
    pub fn as_label(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Disposable => "disposable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmpfsMountConfig {
    pub target: String,
    pub size: MemoryLimit,
    #[serde(default = "default_tmpfs_mode")]
    pub mode: String,
}

fn default_true() -> bool {
    true
}

fn default_tmpfs_mode() -> String {
    "1777".to_string()
}
