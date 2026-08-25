//! VM provider abstraction library.
//!
//! This library provides a unified interface for working with different VM providers
//! such as Docker, Podman, and Tart. It defines core traits and factory functions
//! for provider instantiation and management.

// Standard library
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// External crates
use vm_core::error::Result;

// Internal imports
#[cfg(any(feature = "docker", feature = "tart", feature = "test-helpers"))]
use vm_config::config::ProviderName;
use vm_config::config::{BoxSpec, VmConfig};

// Re-export common types for convenience
pub use capabilities::{CommandProvider, InstanceProvider, ProvisioningProvider, TempProvider};
pub use common::instance::{InstanceInfo, InstanceResolver};
pub use context::ProviderContext;
pub use status::{
    InstanceState, MountUsage, ResourceUsage, RuntimeDiagnostics, ServiceStatus, VmStatusReport,
};
pub use vm_core::error::{Result as VmResult, VmError};

mod capabilities;
pub mod common;
pub mod context;
mod guest_cache;
mod project_plan;
mod resource_limits;
pub mod resources;
mod shell_session;
mod stable_name;
mod status;
pub mod tart;
pub mod tart_base;

// Re-export template constants for testing
pub use resources::THEMES_JSON;
pub use resources::ZSHRC_TEMPLATE;
pub mod security;
pub mod temp_models;

pub mod audio;
pub mod preflight;
mod user_home;

#[cfg(feature = "docker")]
pub mod container;

// When the `test-helpers` feature is enabled, include the mock provider.
#[cfg(feature = "test-helpers")]
pub mod mock;

pub use temp_models::{Mount, MountPermission, TempVmState};

/// Internal representation of box configuration after provider-specific parsing
#[derive(Debug, Clone)]
pub enum BoxConfig {
    /// Docker image from registry (e.g., "ubuntu:24.04")
    DockerImage(String),

    /// Build from Dockerfile
    Dockerfile {
        path: PathBuf,
        context: PathBuf,
        args: Option<HashMap<String, String>>,
    },

    /// Tart OCI image (e.g., "ghcr.io/cirruslabs/macos-sequoia-base:latest")
    TartImage(String),

    /// Snapshot reference (universal across providers)
    Snapshot(String),
}

impl BoxConfig {
    fn looks_like_tart_image(s: &str) -> bool {
        let lower = s.to_ascii_lowercase();
        tart_base::guest_os(s).is_some()
            || s.starts_with(tart_base::LINUX_REGISTRY)
            || lower.contains("cirruslabs/macos")
    }

    fn looks_like_dockerfile_path(s: &str) -> bool {
        let potential_path = Path::new(s);
        let lower = s.to_ascii_lowercase();
        s.starts_with("./")
            || s.starts_with("../")
            || potential_path.is_absolute()
            || lower == "dockerfile"
            || lower.ends_with("/dockerfile")
            || lower.ends_with(".dockerfile")
    }

    /// Parse a BoxSpec for Docker provider
    ///
    /// # Detection Rules
    /// - Starts with `@` → Snapshot
    /// - Starts with `./`, `../`, `/` → Dockerfile path
    /// - Ends with `.dockerfile` → Dockerfile
    /// - Otherwise → Docker image
    pub fn parse_for_docker(spec: &BoxSpec, base_dir: &Path) -> Result<Self> {
        match spec {
            BoxSpec::String(s) => {
                // Snapshot (@prefix)
                if let Some(name) = s.strip_prefix('@') {
                    return Ok(BoxConfig::Snapshot(name.to_string()));
                }

                // Dockerfile (path-like)
                let potential_path = Path::new(s);
                if s.starts_with("./") || s.starts_with("../") || potential_path.is_absolute() {
                    let path = if potential_path.is_absolute() {
                        PathBuf::from(s)
                    } else {
                        base_dir.join(s)
                    };
                    let context = path.parent().unwrap_or(base_dir).to_path_buf();
                    return Ok(BoxConfig::Dockerfile {
                        path,
                        context,
                        args: None,
                    });
                }

                // Dockerfile (.dockerfile extension)
                if s.ends_with(".dockerfile") {
                    let path = base_dir.join(s);
                    let context = base_dir.to_path_buf();
                    return Ok(BoxConfig::Dockerfile {
                        path,
                        context,
                        args: None,
                    });
                }

                if Self::looks_like_tart_image(s) {
                    return Err(VmError::Config(format!(
                        "'{s}' looks like a Tart image, but the Docker provider was selected. Use provider: tart or choose a Docker image/Dockerfile."
                    )));
                }

                // Default: Docker image
                Ok(BoxConfig::DockerImage(s.to_string()))
            }

            BoxSpec::Build {
                dockerfile,
                context,
                args,
            } => {
                // Handle absolute vs relative paths correctly
                let dockerfile_path = Path::new(dockerfile);
                let path = if dockerfile_path.is_absolute() {
                    PathBuf::from(dockerfile)
                } else {
                    base_dir.join(dockerfile)
                };

                let ctx = if let Some(c) = context {
                    let context_path = Path::new(c);
                    if context_path.is_absolute() {
                        PathBuf::from(c)
                    } else {
                        base_dir.join(c)
                    }
                } else {
                    // Default to Dockerfile's parent directory
                    path.parent().unwrap_or(base_dir).to_path_buf()
                };

                Ok(BoxConfig::Dockerfile {
                    path,
                    context: ctx,
                    args: args.clone().map(|m| m.into_iter().collect()),
                })
            }
        }
    }

    /// Parse a BoxSpec for Tart provider
    ///
    /// # Detection Rules
    /// - Starts with `@` → Snapshot
    /// - Otherwise → OCI image
    /// - Build variant → Error (not supported)
    pub fn parse_for_tart(spec: &BoxSpec) -> Result<Self> {
        match spec {
            BoxSpec::String(s) => {
                // Snapshot
                if let Some(name) = s.strip_prefix('@') {
                    return Ok(BoxConfig::Snapshot(name.to_string()));
                }

                if Self::looks_like_dockerfile_path(s) {
                    return Err(VmError::Config(format!(
                        "'{s}' looks like a Dockerfile path, but the Tart provider cannot build Dockerfiles. Use provider: docker or choose a Tart OCI image."
                    )));
                }

                // OCI image
                Ok(BoxConfig::TartImage(s.to_string()))
            }

            BoxSpec::Build { .. } => Err(VmError::Config(
                "Tart provider does not support Dockerfile builds".to_string(),
            )),
        }
    }
}

/// Factory-owned aggregate over the provider capabilities used by the CLI.
pub trait Provider: CommandProvider + InstanceProvider + ProvisioningProvider {
    /// Get access to temp provider capabilities if supported
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        None
    }

    /// Clone the provider into a new Box.
    fn clone_box(&self) -> Box<dyn Provider>;
}

impl Clone for Box<dyn Provider> {
    fn clone(&self) -> Box<dyn Provider> {
        self.clone_box()
    }
}

/// Creates a provider instance based on the configuration.
///
/// # Arguments
/// * `config` - The VM configuration containing provider settings
///
/// # Returns
/// A boxed provider implementation or an error if the provider is unknown.
pub fn get_provider(config: VmConfig) -> Result<Box<dyn Provider>> {
    let provider_name = config.provider.clone().unwrap_or_default();

    #[cfg(feature = "test-helpers")]
    if matches!(provider_name, ProviderName::Mock) {
        return Ok(Box::new(mock::MockProvider::new(config)));
    }

    match &provider_name {
        #[cfg(feature = "docker")]
        ProviderName::Docker | ProviderName::Podman => {
            let engine = container::ContainerEngine::detect(&provider_name)?;
            Ok(Box::new(container::ContainerProvider::new(config, engine)?))
        }
        #[cfg(feature = "tart")]
        ProviderName::Tart => Ok(Box::new(tart::TartProvider::new(config)?)),
        _ => Err(VmError::Provider(format!(
            "Unknown provider: {provider_name}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::VmConfig;

    #[test]
    fn test_get_provider_default_docker() {
        let config = VmConfig::default();
        let result = get_provider(config);
        // Test that we default to docker, even if docker is not available
        match result {
            Ok(provider) => assert_eq!(provider.name(), "docker"),
            Err(error) => {
                // If docker is not available, we should get a dependency error
                assert!(error.to_string().contains("Dependency not found"));
            }
        }
    }

    #[test]
    fn test_get_provider_explicit_docker() {
        let config = VmConfig {
            provider: Some("docker".into()),
            ..Default::default()
        };
        let result = get_provider(config);
        // Test that we try to create docker provider
        match result {
            Ok(provider) => assert_eq!(provider.name(), "docker"),
            Err(error) => {
                // If docker is not available, we should get a dependency error
                assert!(error.to_string().contains("Dependency not found"));
            }
        }
    }

    #[test]
    fn test_get_provider_explicit_podman() {
        let config = VmConfig {
            provider: Some("podman".into()),
            ..Default::default()
        };
        let result = get_provider(config);
        // Test that we try to create podman provider
        match result {
            Ok(provider) => assert_eq!(provider.name(), "podman"),
            Err(error) => {
                // If podman is not available, we should get a dependency error
                assert!(error.to_string().contains("Dependency not found"));
            }
        }
    }

    #[test]
    #[cfg(feature = "tart")]
    fn test_get_provider_explicit_tart() {
        let config = VmConfig {
            provider: Some("tart".into()),
            ..Default::default()
        };
        match get_provider(config) {
            Ok(provider) => assert_eq!(provider.name(), "tart"),
            Err(VmError::Dependency(dependency)) => assert_eq!(dependency, "Tart"),
            Err(error) => panic!("Tart provider was not registered: {error}"),
        }
    }

    #[test]
    #[cfg(feature = "test-helpers")]
    fn test_get_provider_mock() {
        let config = VmConfig {
            provider: Some("mock".into()),
            ..Default::default()
        };
        let provider = get_provider(config).expect("Should create mock provider");
        assert_eq!(provider.name(), "mock");
    }

    #[test]
    fn test_get_provider_unknown() {
        let config = VmConfig {
            provider: Some("unknown-provider".into()),
            ..Default::default()
        };
        let result = get_provider(config);
        assert!(result.is_err());

        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(error_msg.contains("Unknown provider"));
            assert!(error_msg.contains("unknown-provider"));
        }
    }

    #[test]
    fn runtime_states_are_normalized() {
        assert_eq!(
            InstanceState::from_runtime_status("running"),
            InstanceState::Running
        );
        assert_eq!(
            InstanceState::from_runtime_status("exited"),
            InstanceState::Stopped
        );
        assert_eq!(
            InstanceState::from_runtime_status("restarting"),
            InstanceState::Starting
        );
        assert_eq!(
            InstanceState::from_runtime_status("paused"),
            InstanceState::Paused
        );
        assert_eq!(
            InstanceState::from_runtime_status("future-state"),
            InstanceState::Unknown("future-state".to_string())
        );
    }
}
