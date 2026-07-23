use serde::Serialize;
use vm_config::config::{CpuLimit, MemoryLimit, VmConfig, VolumeRetention, VolumeScope};
use vm_core::error::{Result, VmError};

#[derive(Serialize)]
pub(super) struct RenderedResources {
    pub memory: Option<String>,
    pub cpus: Option<u32>,
}

impl RenderedResources {
    pub fn resolve(config: &VmConfig) -> Result<Self> {
        let vm = config.vm.as_ref();
        let memory = match vm.and_then(|settings| settings.memory.as_ref()) {
            Some(MemoryLimit::Limited(megabytes)) => Some(format!("{megabytes}m")),
            Some(limit @ MemoryLimit::Percentage(_)) => {
                let total_megabytes = vm_platform::platform::total_memory_gb()
                    .map_err(|error| {
                        VmError::Internal(format!("Failed to resolve host memory: {error}"))
                    })?
                    .saturating_mul(1024);
                limit
                    .resolve_percentage(total_megabytes)
                    .map(|megabytes| format!("{megabytes}m"))
            }
            Some(MemoryLimit::Unlimited) | None => None,
        };
        let cpus = match vm.and_then(|settings| settings.cpus.as_ref()) {
            Some(CpuLimit::Limited(count)) => Some(*count),
            Some(limit @ CpuLimit::Percentage(_)) => {
                let available = vm_platform::platform::cpu_core_count().map_err(|error| {
                    VmError::Internal(format!("Failed to resolve host CPU count: {error}"))
                })?;
                limit.resolve_percentage(available)
            }
            Some(CpuLimit::Unlimited) | None => None,
        };

        Ok(Self { memory, cpus })
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
}

impl RenderedStorage {
    pub fn new(config: &VmConfig, base_project: &str, instance_project: &str) -> Self {
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
    use vm_config::config::VmSettings;

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
