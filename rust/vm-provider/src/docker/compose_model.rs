use serde::Serialize;
use vm_config::config::{VmConfig, VolumeRetention, VolumeScope};
use vm_core::error::Result;

use crate::resource_limits::ResolvedResources;

#[derive(Serialize)]
pub(super) struct RenderedResources {
    pub memory: Option<String>,
    pub cpus: Option<u32>,
}

impl RenderedResources {
    pub fn resolve(config: &VmConfig) -> Result<Self> {
        let resources = ResolvedResources::resolve(config)?;
        let memory = resources.memory_mb.map(|megabytes| format!("{megabytes}m"));

        Ok(Self {
            memory,
            cpus: resources.cpus,
        })
    }
}

#[derive(Clone, Serialize)]
pub(super) struct RenderedVolume {
    pub alias: String,
    pub name: String,
    pub target: Option<String>,
    pub nocopy: bool,
    pub retention: &'static str,
}

#[derive(Serialize)]
pub(super) struct RenderedTmpfs {
    pub target: String,
    pub size: Option<String>,
    pub mode: String,
}

pub(super) struct RenderedStorage {
    pub mounts: Vec<RenderedVolume>,
    pub named_volumes: Vec<RenderedVolume>,
    pub tmpfs: Vec<RenderedTmpfs>,
    pub tool_cache_target: Option<String>,
}

impl RenderedStorage {
    pub fn new(
        config: &VmConfig,
        base_project: &str,
        instance_project: &str,
        tool_cache_target: &str,
    ) -> Self {
        let mounts = config
            .storage
            .volumes
            .iter()
            .map(|(logical_name, volume)| {
                let scope = match volume.scope {
                    VolumeScope::Project => base_project.to_string(),
                    VolumeScope::Instance => instance_project.to_string(),
                    VolumeScope::Platform => {
                        format!("{base_project}_linux_{}", container_architecture())
                    }
                };
                RenderedVolume {
                    alias: format!("managed_{}", stable_name_component(logical_name)),
                    name: stable_volume_name(&scope, logical_name),
                    target: Some(volume.target.clone()),
                    nocopy: volume.nocopy,
                    retention: volume.retention.as_label(),
                }
            })
            .collect::<Vec<_>>();
        let mut named_volumes = mounts.clone();
        named_volumes.push(builtin_volume(instance_project, "shell_history"));
        let tool_cache_target = (!config
            .storage
            .volumes
            .values()
            .any(|volume| volume.target == tool_cache_target))
        .then(|| tool_cache_target.to_string());
        if tool_cache_target.is_some() {
            let platform_scope = format!("{base_project}_linux_{}", container_architecture());
            named_volumes.push(builtin_volume(&platform_scope, "tool_cache"));
        }
        if config
            .services
            .get("postgresql")
            .is_some_and(|service| service.enabled)
        {
            named_volumes.push(builtin_volume(instance_project, "postgres_data"));
        }

        let mut tmpfs = config
            .storage
            .tmpfs
            .iter()
            .filter_map(|mount| {
                Some(RenderedTmpfs {
                    target: mount.target.clone(),
                    size: Some(format!("{}m", mount.size.to_mb()?)),
                    mode: mount.mode.clone(),
                })
            })
            .collect::<Vec<_>>();
        if config
            .security
            .as_ref()
            .is_some_and(|security| security.read_only_root)
        {
            for target in ["/tmp", "/var/tmp"] {
                if !tmpfs.iter().any(|mount| mount.target == target) {
                    tmpfs.push(RenderedTmpfs {
                        target: target.to_string(),
                        size: None,
                        mode: "1777".to_string(),
                    });
                }
            }
        }

        Self {
            mounts,
            named_volumes,
            tmpfs,
            tool_cache_target,
        }
    }
}

fn builtin_volume(project: &str, logical_name: &str) -> RenderedVolume {
    RenderedVolume {
        alias: logical_name.to_string(),
        name: stable_volume_name(project, logical_name),
        target: None,
        nocopy: true,
        retention: VolumeRetention::Keep.as_label(),
    }
}

fn stable_volume_name(scope: &str, logical_name: &str) -> String {
    format!(
        "vm_{}_{}",
        stable_name_component(scope),
        stable_name_component(logical_name)
    )
}

pub(super) fn stable_name_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn instance_name_from_container(
    project_name: &str,
    container_name: &str,
) -> Option<String> {
    container_name
        .strip_prefix(&format!("{project_name}-"))?
        .strip_suffix("-dev")
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

pub(super) fn container_architecture() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        architecture => architecture,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::{
        CpuLimit, MemoryLimit, VmSettings, VolumeMountConfig, VolumeRetention, VolumeScope,
    };

    #[test]
    fn resolves_fixed_resource_limits_for_compose() {
        let config = VmConfig {
            vm: Some(VmSettings {
                memory: Some(MemoryLimit::Limited(8192)),
                cpus: Some(CpuLimit::Limited(6)),
                ..Default::default()
            }),
            ..Default::default()
        };

        let resources = RenderedResources::resolve(&config).unwrap();
        assert_eq!(resources.memory.as_deref(), Some("8192m"));
        assert_eq!(resources.cpus, Some(6));
    }

    #[test]
    fn explicit_cache_mount_replaces_the_builtin_tool_cache() {
        let mut config = VmConfig::default();
        config.storage.volumes.insert(
            "custom_cache".to_string(),
            VolumeMountConfig {
                target: "/home/developer/.cache".to_string(),
                scope: VolumeScope::Project,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );

        let storage =
            RenderedStorage::new(&config, "codeatlas", "codeatlas", "/home/developer/.cache");

        assert!(storage.tool_cache_target.is_none());
        assert!(!storage
            .named_volumes
            .iter()
            .any(|volume| volume.alias == "tool_cache"));
    }

    #[test]
    fn resolves_only_named_dev_container_suffixes() {
        assert_eq!(
            instance_name_from_container("sketch-api", "sketch-api-feature-dev").as_deref(),
            Some("feature")
        );
        assert_eq!(
            instance_name_from_container("sketch-api", "sketch-api-dev"),
            None
        );
    }
}
