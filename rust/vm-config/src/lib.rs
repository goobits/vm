//! VM configuration management library.
//!
//! This library provides functionality for managing VM configurations, including
//! loading, merging, validating configurations, and managing presets.
//!
//! ## Main Features
//! - Configuration loading and validation
//! - Configuration merging and preset management
//! - Project configuration initialization
//! - Resource limit parsing and validation
//!
//! ## Preset System
//!
//! The configuration system supports two types of presets:
//!
//! ### Image Presets
//!
//! Pre-built Docker images with all tooling pre-installed.
//! Creates minimal `vm.yaml` with just the image reference.
//!
//! ### Provision Presets
//!
//! Package manifests installed at runtime. Used via `vm config preset <preset>`.
//! Merges package lists into existing `vm.yaml`.
//!
//! Preset loading and discovery is handled internally by the `preset` module.

pub mod config;
mod config_ops;
pub mod detector;
mod embedded_presets;
mod global_config;
mod limit_parser;
mod loader;
mod merge;
pub mod ports;
mod preset;
mod schema;
pub mod validation;
mod yaml;

#[cfg(test)]
mod test_memory;

#[cfg(test)]
mod global_config_tests;

#[cfg(test)]
mod config_tests;

// Re-export config operations for use by main vm binary
pub use config_ops::{init_config_file, ConfigOps};

// Re-export global config for use by other crates
pub use global_config::{
    AuthProxySettings, BackupSettings, GlobalConfig, GlobalDefaults, GlobalFeatures,
    GlobalServices, MongoDBSettings, MySqlSettings, PackageInfrastructureSettings,
    PostgresSettings, RedisSettings, SnapshotSettings, WorktreesGlobalSettings,
};

pub use detector::{detect_worktrees, detect_worktrees_in};

// Re-export ConfigLoader for relative path detection
pub use loader::ConfigLoader;
pub use merge::{apply_profile, merge_configs, ConfigMerger};
pub use preset::PresetDetector;
pub use yaml::CoreOperations;

use std::path::PathBuf;
use vm_core::error::Result;
use vm_core::error::VmError;

/// Complete application configuration containing both global and VM-specific settings
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Global configuration from ~/.vm/config.yaml
    pub global: GlobalConfig,
    /// VM-specific configuration from vm.yaml
    pub vm: config::VmConfig,
}

impl AppConfig {
    /// Container provider effective for commands in the current project.
    pub fn container_provider(&self) -> config::ProviderName {
        self.vm
            .provider
            .as_ref()
            .filter(|provider| provider.is_container())
            .cloned()
            .unwrap_or_else(|| self.global.container_provider())
    }

    /// Resolve the effective profile without prompting or mutating configuration.
    pub fn resolve_profile_name(
        vm: &config::VmConfig,
        explicit_profile: Option<&str>,
        provider_override: Option<&str>,
    ) -> Option<String> {
        if let Some(profile) = explicit_profile {
            return Some(profile.to_string());
        }

        let effective_provider =
            provider_override.or_else(|| vm.provider.as_ref().map(config::ProviderName::as_str));
        if let Some(provider_name) = effective_provider {
            if vm
                .profiles
                .as_ref()
                .is_some_and(|profiles| profiles.contains_key(provider_name))
            {
                return Some(provider_name.to_string());
            }
        }

        vm.default_profile.clone().or_else(|| {
            vm.profiles
                .as_ref()
                .filter(|profiles| profiles.len() == 1)
                .and_then(|profiles| profiles.keys().next().cloned())
        })
    }

    /// Load complete configuration from standard locations
    ///
    /// This is the main entry point for loading all configuration. It:
    /// 1. Loads global config from ~/.vm/config.yaml
    /// 2. Loads VM config from provided path or auto-discovers
    /// 3. Applies defaults and presets
    /// 4. Merges configurations in proper precedence order
    pub fn load(
        config_path: Option<PathBuf>,
        profile: Option<String>,
        provider_override: Option<String>,
    ) -> Result<Self> {
        // Load global configuration
        let global = GlobalConfig::load()
            .map_err(|e| VmError::Config(format!("Failed to load global configuration: {e}")))?;

        // Load VM configuration with all merging logic
        let mut vm = config::VmConfig::load(config_path.clone())?;
        let source_path = vm.source_path.clone();

        // Apply profile if specified
        let profile_name =
            Self::resolve_profile_name(&vm, profile.as_deref(), provider_override.as_deref());
        if let Some(profile_name) = profile_name {
            vm = merge::apply_profile(vm, &profile_name)?;
            vm.source_path = source_path;
        }

        if let Some(provider_name) = provider_override {
            vm.provider = Some(provider_name.into());
        }

        let global_tools = config::ToolsConfig {
            entries: global.tools.clone(),
            ..Default::default()
        };
        global_tools.validate()?;
        for (name, tool) in global_tools.entries {
            vm.tools.entries.entry(name).or_insert(tool);
        }

        // Handle host integrations
        let should_copy_git = vm
            .host_sync
            .as_ref()
            .map(|hs| hs.git_config)
            .unwrap_or(true); // Default: true

        if should_copy_git {
            vm.git_config = Some(detector::git::detect_git_config()?);
        }

        if vm.vm.as_ref().and_then(|v| v.timezone.as_deref()) == Some("auto") {
            let detected_timezone = vm_platform::platform::detect_timezone();
            if let Some(vm_settings) = vm.vm.as_mut() {
                vm_settings.timezone = Some(detected_timezone);
            }
        }

        Ok(Self { global, vm })
    }
}

#[cfg(test)]
mod app_config_tests {
    use super::AppConfig;
    use crate::{config::ProviderName, config::ToolConfig, GlobalConfig};
    use serial_test::serial;
    use vm_core::error::Result;

    #[test]
    fn project_container_provider_overrides_the_global_default() {
        let mut config = AppConfig {
            global: GlobalConfig::default(),
            vm: Default::default(),
        };
        config.global.defaults.provider = Some("podman".to_string());
        assert_eq!(config.container_provider(), ProviderName::Podman);

        config.vm.provider = Some(ProviderName::Docker);
        assert_eq!(config.container_provider(), ProviderName::Docker);

        config.vm.provider = Some(ProviderName::Tart);
        assert_eq!(config.container_provider(), ProviderName::Podman);
    }

    fn with_temp_home<T>(test: impl FnOnce(&tempfile::TempDir) -> Result<T>) -> Result<T> {
        let temp_dir = tempfile::TempDir::new()?;
        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", temp_dir.path());
        let result = test(&temp_dir);
        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
        result
    }

    #[test]
    #[serial]
    fn global_tools_apply_to_projects_and_project_settings_win() -> Result<()> {
        with_temp_home(|temp_dir| {
            let mut global = GlobalConfig::default();
            global.tools.insert(
                "codeatlas".into(),
                ToolConfig {
                    version: Some("0.10.0".into()),
                    ..Default::default()
                },
            );
            global
                .tools
                .insert("typemill".into(), ToolConfig::default());
            global.save()?;

            let config_path = temp_dir.path().join("vm.yaml");
            std::fs::write(
                &config_path,
                "provider: docker\ntools:\n  codeatlas:\n    version: 0.11.0\n",
            )?;

            let app = AppConfig::load(Some(config_path), None, None)?;
            assert_eq!(
                app.vm.tools.entries["codeatlas"].version.as_deref(),
                Some("0.11.0")
            );
            assert!(app.vm.tools.entries.contains_key("typemill"));
            Ok(())
        })
    }

    #[test]
    #[serial]
    fn provider_override_uses_matching_profile_when_present() -> Result<()> {
        with_temp_home(|temp_dir| {
            let config_path = temp_dir.path().join("vm.yaml");
            std::fs::write(
                &config_path,
                r#"
provider: docker
default_profile: docker
profiles:
  docker:
    provider: docker
    vm:
      image: "@vibe-image"
  tart:
    provider: tart
    vm:
      image: vibe-tart-sequoia-base
"#,
            )?;

            let app = AppConfig::load(Some(config_path), None, Some("tart".to_string()))?;
            assert_eq!(app.vm.provider.as_deref(), Some("tart"));
            assert_eq!(
                app.vm
                    .vm
                    .as_ref()
                    .and_then(|vm| vm.image.as_ref())
                    .map(|b| serde_yaml_ng::to_string(b).unwrap().trim().to_string()),
                Some("vibe-tart-sequoia-base".to_string())
            );
            Ok(())
        })
    }

    #[test]
    #[serial]
    fn configured_provider_uses_matching_profile_when_present() -> Result<()> {
        with_temp_home(|temp_dir| {
            let config_path = temp_dir.path().join("vm.yaml");
            std::fs::write(
                &config_path,
                r#"
provider: tart
default_profile: docker
profiles:
  docker:
    provider: docker
    vm:
      image: "@vibe-image"
  tart:
    provider: tart
    vm:
      image: vibe-tart-sequoia-base
"#,
            )?;

            let app = AppConfig::load(Some(config_path), None, None)?;
            assert_eq!(app.vm.provider.as_deref(), Some("tart"));
            assert_eq!(
                app.vm
                    .vm
                    .as_ref()
                    .and_then(|vm| vm.image.as_ref())
                    .map(|b| serde_yaml_ng::to_string(b).unwrap().trim().to_string()),
                Some("vibe-tart-sequoia-base".to_string())
            );
            Ok(())
        })
    }

    #[test]
    #[serial]
    fn explicit_profile_takes_precedence_over_provider_override() -> Result<()> {
        with_temp_home(|temp_dir| {
            let config_path = temp_dir.path().join("vm.yaml");
            std::fs::write(
                &config_path,
                r#"
provider: docker
default_profile: docker
profiles:
  docker:
    provider: docker
    vm:
      image: "@vibe-image"
  tart:
    provider: tart
    vm:
      image: vibe-tart-sequoia-base
"#,
            )?;

            let app = AppConfig::load(
                Some(config_path),
                Some("docker".to_string()),
                Some("tart".to_string()),
            )?;
            assert_eq!(app.vm.provider.as_deref(), Some("tart"));
            assert_eq!(
                app.vm
                    .vm
                    .as_ref()
                    .and_then(|vm| vm.image.as_ref())
                    .map(|b| serde_yaml_ng::to_string(b).unwrap().trim().to_string()),
                Some("'@vibe-image'".to_string())
            );
            Ok(())
        })
    }

    #[test]
    #[serial]
    fn sole_profile_is_selected_without_an_explicit_default() -> Result<()> {
        with_temp_home(|temp_dir| {
            let config_path = temp_dir.path().join("vm.yaml");
            std::fs::write(
                &config_path,
                r#"
profiles:
  container:
    provider: docker
"#,
            )?;

            let app = AppConfig::load(Some(config_path), None, None)?;
            assert_eq!(app.vm.provider.as_deref(), Some("docker"));
            Ok(())
        })
    }
}
