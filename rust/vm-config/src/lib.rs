//! VM configuration management library.
//!
//! This library provides functionality for managing VM configurations, including
//! loading, merging, validating configurations, and managing presets.
//!
//! ## Main Features
//! - Configuration loading and validation
//! - Configuration merging and preset management
//! - CLI utilities for configuration initialization
//! - Resource management utilities
//!
//! ## Preset System
//!
//! The configuration system supports two types of presets:
//!
//! ### Box Presets
//!
//! Pre-built Docker images with all tooling pre-installed. Used via `vm init <preset>`.
//! Creates minimal `vm.yaml` with just the box reference.
//!
//! ### Provision Presets
//!
//! Package manifests installed at runtime. Used via `vm config preset <preset>`.
//! Merges package lists into existing `vm.yaml`.
//!
//! Preset loading and discovery is handled internally by the `preset` module.

pub mod cli;
pub mod config;
pub mod config_ops;
pub mod detector;
mod embedded_presets;
pub mod global_config;
pub mod limit_parser; // Shared limit parsing logic
pub mod loader;
pub mod merge;
pub mod paths;
pub mod ports;
pub mod preset; // Made public for integration tests - used internally by config_ops and cli
pub mod preset_cache; // Preset caching layer
pub mod resources; // VM resource suggestions
pub mod schema; // Schema-aware type detection
pub mod validate;
pub mod validator;
pub mod yaml; // YAML operations module

#[cfg(test)]
mod test_memory;

#[cfg(test)]
mod global_config_tests;

#[cfg(test)]
mod config_tests;

// Re-export commonly needed path utilities
pub use paths::{
    get_current_gid, get_current_uid, get_presets_dir, get_tool_dir, resolve_tool_path,
};

// Re-export config operations for use by main vm binary
pub use config_ops::{load_global_config, ConfigOps};

// Re-export global config for use by other crates
pub use global_config::GlobalConfig;

// Re-export CLI utilities
pub use cli::init_config_file;
pub use detector::detect_worktrees;

// Re-export ConfigLoader for relative path detection
pub use loader::ConfigLoader;

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
    fn resolve_profile_name(
        vm: &config::VmConfig,
        explicit_profile: Option<String>,
        provider_override: Option<String>,
    ) -> Option<String> {
        if explicit_profile.is_some() {
            return explicit_profile;
        }

        let effective_provider = provider_override.or_else(|| vm.provider.clone());
        if let Some(provider_name) = effective_provider {
            if vm
                .profiles
                .as_ref()
                .is_some_and(|profiles| profiles.contains_key(&provider_name))
            {
                return Some(provider_name);
            }
        }

        vm.default_profile.clone()
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
            Self::resolve_profile_name(&vm, profile.clone(), provider_override.clone());
        if let Some(profile_name) = profile_name {
            vm = merge::apply_profile(vm, &profile_name)?;
            vm.source_path = source_path;
        }

        if let Some(provider_name) = provider_override {
            vm.provider = Some(provider_name);
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
            let detected_timezone = detector::os::detect_timezone();
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
    use vm_core::error::Result;

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
      box: "@vibe-box"
  tart:
    provider: tart
    vm:
      box: vibe-tart-base
"#,
            )?;

            let app = AppConfig::load(Some(config_path), None, Some("tart".to_string()))?;
            assert_eq!(app.vm.provider.as_deref(), Some("tart"));
            assert_eq!(
                app.vm
                    .vm
                    .as_ref()
                    .and_then(|vm| vm.r#box.as_ref())
                    .map(|b| serde_yaml_ng::to_string(b).unwrap().trim().to_string()),
                Some("vibe-tart-base".to_string())
            );
            Ok(())
        })
    }

    #[test]
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
      box: "@vibe-box"
  tart:
    provider: tart
    vm:
      box: vibe-tart-base
"#,
            )?;

            let app = AppConfig::load(Some(config_path), None, None)?;
            assert_eq!(app.vm.provider.as_deref(), Some("tart"));
            assert_eq!(
                app.vm
                    .vm
                    .as_ref()
                    .and_then(|vm| vm.r#box.as_ref())
                    .map(|b| serde_yaml_ng::to_string(b).unwrap().trim().to_string()),
                Some("vibe-tart-base".to_string())
            );
            Ok(())
        })
    }
}
