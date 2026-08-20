use std::fs;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml_ng as serde_yaml;
use vm_core::error::Result;

use crate::detector::git::GitConfig;
pub use crate::ports::PortsConfig;

mod environment;
mod host_sync;
mod limits;
pub mod mounts;
mod runtime;
mod storage;
pub mod tools;

#[cfg(feature = "test-helpers")]
pub use environment::{MockProviderConfig, MockVmInstanceConfig, VmStatusReportConfig};
pub use environment::{
    NetworkingConfig, PackageEdgeConfig, ProjectConfig, SecurityConfig, ServiceConfig, TartConfig,
    TerminalConfig,
};
pub use host_sync::{AiSyncConfig, AiSyncTools, HostSyncConfig, WorktreesConfig};
pub use limits::{CpuLimit, DiskLimit, MemoryLimit, SwapLimit};
pub use mounts::{MountAccess, MountConfig};
pub use runtime::{
    BootstrapConfig, BoxSpec, ContainerLoggingConfig, PlaywrightBootstrapConfig, VersionsConfig,
    VmSettings,
};
pub use storage::{
    StorageConfig, TmpfsMountConfig, VolumeMountConfig, VolumeRetention, VolumeScope,
};
pub use tools::{ToolConfig, ToolUpdatePolicy, ToolsConfig};

/// Main VM configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VmConfig {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tart: Option<TartConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vm: Option<VmSettings>,
    #[serde(default, skip_serializing_if = "StorageConfig::is_empty")]
    pub storage: StorageConfig,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<MountConfig>,
    #[serde(default, skip_serializing_if = "ToolsConfig::is_empty")]
    pub tools: ToolsConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions: Option<VersionsConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootstrap: Option<BootstrapConfig>,
    #[serde(default)]
    pub ports: PortsConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub networking: Option<NetworkingConfig>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalConfig>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub aliases: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub environment: IndexMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host_sync: Option<HostSyncConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<SecurityConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profiles: Option<IndexMap<String, VmConfig>>,
    #[serde(flatten)]
    pub extra_config: IndexMap<String, serde_json::Value>,
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
    #[serde(skip)]
    pub git_config: Option<GitConfig>,
    #[serde(skip)]
    pub package_edge: Option<PackageEdgeConfig>,
    #[cfg(feature = "test-helpers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mock: Option<MockProviderConfig>,
}

impl VmConfig {
    pub fn load(file: Option<PathBuf>) -> Result<Self> {
        let mut config = crate::cli::load_and_merge_config(file)?;
        config.apply_default_backup_settings();
        Ok(config)
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        fs::write(path, serde_yaml::to_string(self)?)?;
        Ok(())
    }

    /// Resolve the project root without ever treating the global VM state
    /// directory as a project workspace.
    pub fn project_dir(&self) -> Result<PathBuf> {
        let current_dir = || Ok(std::env::current_dir()?);
        let Some(source) = self.source_path.as_deref() else {
            return current_dir();
        };
        if source == vm_core::user_paths::global_config_path()?.as_path() {
            return current_dir();
        }
        Ok(if source.is_dir() {
            source.to_path_buf()
        } else {
            source
                .parent()
                .map_or_else(|| source.to_path_buf(), Path::to_path_buf)
        })
    }

    /// Return the project-owned configuration path, excluding global state.
    pub fn owning_config_path(&self) -> Option<&Path> {
        let source = self.source_path.as_deref()?;
        match vm_core::user_paths::global_config_path() {
            Ok(global) if global == source => None,
            Ok(_) => Some(source),
            Err(_) => None,
        }
    }

    pub fn from_file(path: &PathBuf) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        crate::yaml::CoreOperations::parse_yaml_with_diagnostics(
            &content,
            &path.display().to_string(),
        )
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn apply_default_backup_settings(&mut self) {
        for service in self.services.values_mut() {
            if service.backup_on_destroy.is_none() && service.r#type.as_deref() == Some("database")
            {
                service.backup_on_destroy = Some(true);
            }
        }
    }

    pub fn is_partial(&self) -> bool {
        self.provider.is_none()
            || self
                .project
                .as_ref()
                .map_or(true, |project| project.name.is_none())
    }

    pub fn ensure_service_ports(&mut self) {
        const PRIORITY: &[&str] = &["postgresql", "redis", "mysql", "mongodb"];
        const WITHOUT_PORTS: &[&str] = &["docker"];
        let Some(range) = self.ports.range.as_ref().filter(|range| range.len() == 2) else {
            return;
        };
        let (start, end) = (range[0], range[1]);
        let mut used = self
            .services
            .values()
            .filter_map(|service| service.port)
            .collect::<std::collections::HashSet<_>>();
        let mut next = end;
        let mut allocate = || {
            while next >= start {
                let candidate = next;
                next = if next == start { 0 } else { next - 1 };
                if used.insert(candidate) {
                    return Some(candidate);
                }
                if next == 0 {
                    break;
                }
            }
            None
        };

        let mut pending = PRIORITY
            .iter()
            .filter(|name| {
                self.services
                    .get(**name)
                    .is_some_and(|service| service.enabled && service.port.is_none())
            })
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>();
        let mut remaining = self
            .services
            .iter()
            .filter(|(name, service)| {
                service.enabled
                    && service.port.is_none()
                    && !PRIORITY.contains(&name.as_str())
                    && !WITHOUT_PORTS.contains(&name.as_str())
            })
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        remaining.sort();
        pending.extend(remaining);

        for name in pending {
            if let Some(port) = allocate() {
                self.services.get_mut(&name).expect("known service").port = Some(port);
            }
        }
        for service in self.services.values_mut() {
            if !service.enabled
                && service
                    .port
                    .is_some_and(|port| (start..=end).contains(&port))
            {
                service.port = None;
            }
        }
    }
}

#[cfg(test)]
mod container_policy_tests {
    use super::{MemoryLimit, VmConfig, VolumeRetention, VolumeScope};
    use serial_test::serial;

    #[test]
    #[serial]
    fn project_dir_never_uses_the_global_config_directory() {
        let mut config = VmConfig {
            source_path: Some(vm_core::user_paths::global_config_path().unwrap()),
            ..Default::default()
        };
        assert_eq!(
            config.project_dir().unwrap(),
            std::env::current_dir().unwrap()
        );
        assert_eq!(config.owning_config_path(), None);

        config.source_path = Some(std::env::current_dir().unwrap().join("nested/vm.yaml"));
        assert_eq!(
            config.project_dir().unwrap(),
            std::env::current_dir().unwrap().join("nested")
        );
        assert_eq!(config.owning_config_path(), config.source_path.as_deref());
    }

    #[test]
    fn retired_nested_configuration_is_rejected() {
        for yaml in [
            "tart:\n  image: obsolete\n",
            "host_sync:\n  ai_tools:\n    gemini: false\n",
        ] {
            assert!(serde_yaml_ng::from_str::<VmConfig>(yaml).is_err());
        }
    }

    #[test]
    fn parses_container_storage_and_runtime_policy() {
        let config: VmConfig = serde_yaml_ng::from_str(
            "provider: docker\nproject:\n  name: sketch-api\nvm:\n  pids_limit: 4096\n  stop_grace_period: 60\n  logging: {}\nstorage:\n  volumes:\n    node_modules:\n      target: /workspace/node_modules\n      scope: instance\n  tmpfs:\n    - target: /tmp\n      size: 4g\nbootstrap:\n  playwright:\n    browsers: [chromium, firefox, webkit]\n",
        )
        .unwrap();
        let vm = config.vm.unwrap();
        assert_eq!(vm.pids_limit, Some(4096));
        assert_eq!(vm.stop_grace_period, Some(60));
        let logging = vm.logging.unwrap();
        assert_eq!(
            (
                logging.driver.as_str(),
                logging.max_size.as_str(),
                logging.max_files
            ),
            ("local", "20m", 5)
        );
        let volume = &config.storage.volumes["node_modules"];
        assert_eq!(volume.scope, VolumeScope::Instance);
        assert!(volume.nocopy);
        assert_eq!(volume.retention, VolumeRetention::Keep);
        assert_eq!(config.storage.tmpfs[0].size, MemoryLimit::Limited(4096));
        assert_eq!(config.storage.tmpfs[0].mode, "1777");
        assert!(config.bootstrap.unwrap().dependencies);
    }
}
