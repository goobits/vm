mod network;
mod project;
mod runtime;
mod storage;

use crate::config::VmConfig;
use vm_core::error::Result;

struct StructuralValidator<'a> {
    config: &'a VmConfig,
}

impl<'a> StructuralValidator<'a> {
    fn new(config: &'a VmConfig) -> Self {
        Self { config }
    }

    fn validate(&self) -> Result<()> {
        project::validate_required_fields(self.config)?;
        project::validate_provider(self.config)?;
        project::validate_image_spec_compat(self.config)?;
        project::validate_project(self.config)?;
        network::validate_ports(self.config)?;
        network::validate_services(self.config)?;
        runtime::validate_versions(self.config)?;
        network::validate_networking(self.config)?;
        runtime::validate_runtime(self.config)?;
        runtime::validate_resource_limits(self.config)?;
        runtime::validate_bootstrap(self.config)?;
        storage::validate_mounts(self.config)?;
        self.config.tools.validate()?;
        storage::validate_storage(self.config)
    }
}

pub(super) fn validate(config: &VmConfig) -> Result<()> {
    StructuralValidator::new(config).validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BootstrapConfig, MemoryLimit, PlaywrightBootstrapConfig, StorageConfig, TmpfsMountConfig,
        VolumeMountConfig, VolumeRetention, VolumeScope,
    };

    fn validate_owned(config: VmConfig) -> Result<()> {
        validate(&config)
    }

    #[test]
    fn test_valid_config() {
        let mut config = VmConfig::default();
        config.provider = Some("docker".into());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test-project".to_string()),
            hostname: Some("test.local".to_string()),
            workspace_path: Some(
                crate::paths::get_default_workspace_path()
                    .to_string_lossy()
                    .to_string(),
            ),
            workspace_access: Default::default(),
            backup_pattern: None,
            env_template_path: None,
        });

        assert!(validate_owned(config).is_ok());
    }

    #[test]
    fn test_invalid_provider() {
        let mut config = VmConfig::default();
        config.provider = Some("invalid".into());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test".to_string()),
            ..Default::default()
        });

        assert!(validate_owned(config).is_err());
    }

    #[test]
    fn tart_ssh_user_is_structurally_validated() {
        let config = |user: &str| VmConfig {
            provider: Some("tart".into()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            tart: Some(crate::config::TartConfig {
                ssh_user: Some(user.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        assert!(validate_owned(config("admin")).is_ok());
        assert!(validate_owned(config("-oProxyCommand=bad")).is_err());
    }

    #[test]
    fn test_invalid_port_range() {
        let mut config = VmConfig::default();
        config.provider = Some("docker".into());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test".to_string()),
            ..Default::default()
        });
        config.ports.range = Some(vec![0, 10]); // Port 0 is invalid

        assert!(validate_owned(config).is_err());
    }

    #[test]
    fn test_single_port_range_is_valid() {
        let mut config = VmConfig::default();
        config.provider = Some("docker".into());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test".to_string()),
            ..Default::default()
        });
        config.ports.range = Some(vec![3320, 3320]);

        assert!(validate_owned(config).is_ok());
    }

    #[test]
    fn test_reversed_port_range_is_invalid() {
        let mut config = VmConfig::default();
        config.provider = Some("docker".into());
        config.project = Some(crate::config::ProjectConfig {
            name: Some("test".to_string()),
            ..Default::default()
        });
        config.ports.range = Some(vec![3321, 3320]);

        assert!(validate_owned(config).is_err());
    }

    #[test]
    fn test_valid_container_storage_policy() {
        let mut config = VmConfig {
            provider: Some("docker".into()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        config.storage.volumes.insert(
            "node_modules".to_string(),
            VolumeMountConfig {
                target: "/workspace/node_modules".to_string(),
                scope: VolumeScope::Instance,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );
        config.storage.tmpfs.push(TmpfsMountConfig {
            target: "/tmp".to_string(),
            size: MemoryLimit::Limited(4096),
            mode: "1777".to_string(),
        });

        assert!(validate_owned(config).is_ok());
    }

    #[test]
    fn test_storage_policy_cannot_hide_workspace_bind() {
        let mut volumes = indexmap::IndexMap::new();
        volumes.insert(
            "source".to_string(),
            VolumeMountConfig {
                target: "/workspace".to_string(),
                scope: VolumeScope::Project,
                nocopy: true,
                retention: VolumeRetention::Keep,
            },
        );
        let config = VmConfig {
            provider: Some("docker".into()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            storage: StorageConfig {
                volumes,
                ..Default::default()
            },
            ..Default::default()
        };

        let error = validate_owned(config).unwrap_err();
        assert!(error.to_string().contains("cannot replace the /workspace"));
    }

    #[test]
    fn test_storage_policy_rejects_reserved_names_and_unnormalized_targets() {
        let base = || VmConfig {
            provider: Some("docker".into()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let volume = |target: &str| VolumeMountConfig {
            target: target.to_string(),
            scope: VolumeScope::Project,
            nocopy: true,
            retention: VolumeRetention::Keep,
        };

        let mut reserved = base();
        reserved.storage.volumes.insert(
            "shell_history".to_string(),
            volume("/home/developer/history-copy"),
        );
        let error = validate_owned(reserved).unwrap_err();
        assert!(error.to_string().contains("reserved"));

        let mut unnormalized = base();
        unnormalized
            .storage
            .volumes
            .insert("cache".to_string(), volume("/home/developer//cache"));
        let error = validate_owned(unnormalized).unwrap_err();
        assert!(error.to_string().contains("normalized absolute path"));
    }

    #[test]
    fn validates_relative_multi_mounts_and_rejects_target_collisions() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("auth")).unwrap();
        std::fs::create_dir(root.path().join("ui")).unwrap();
        let mut config: VmConfig = serde_yaml_ng::from_str(
            r#"
provider: docker
project:
  name: test
  workspace_path: /source
  workspace_access: read_only
mounts:
  - source: auth
    target: /packages/auth
    access: read_only
  - source: ui
    target: /packages/ui
"#,
        )
        .unwrap();
        config.source_path = Some(root.path().join("vm.yaml"));

        assert!(validate_owned(config.clone()).is_ok());

        config.mounts[1].target = std::path::PathBuf::from("/packages/auth");
        let error = validate_owned(config).unwrap_err();
        assert!(error.to_string().contains("Duplicate mount target"));
    }

    #[test]
    fn test_bootstrap_rejects_unsafe_or_duplicate_browser_names() {
        let base = || VmConfig {
            provider: Some("docker".into()),
            project: Some(crate::config::ProjectConfig {
                name: Some("test".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let with_browsers = |browsers: &[&str]| {
            let mut config = base();
            config.bootstrap = Some(BootstrapConfig {
                dependencies: true,
                playwright: PlaywrightBootstrapConfig {
                    browsers: browsers
                        .iter()
                        .map(|browser| (*browser).to_string())
                        .collect(),
                },
            });
            config
        };

        for browsers in [&["chromium; false"] as &[_], &["webkit", "webkit"]] {
            assert!(validate_owned(with_browsers(browsers)).is_err());
        }
    }
}
