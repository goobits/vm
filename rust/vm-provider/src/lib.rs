//! VM provider abstraction library.
//!
//! This library provides a unified interface for working with different VM providers
//! such as Docker, Podman, and Tart. It defines core traits and factory functions
//! for provider instantiation and management.

// External crates
use vm_core::error::Result;

// Internal imports
use vm_config::config::ProviderName;
use vm_config::config::VmConfig;

// Re-export common types for convenience
pub use capabilities::{
    CommandProvider, InstanceProvider, ProvisioningProvider, TempProvider, TunnelProvider,
};
#[cfg(feature = "docker")]
pub use container::{render_compose_preview, ContainerEngine};
pub use context::ProviderContext;
pub use instance::InstanceInfo;
#[cfg(feature = "test-helpers")]
pub use mock::MockProvider;
pub use status::{
    InstanceState, MountUsage, ResourceUsage, RuntimeDiagnostics, ServiceStatus, VmStatusReport,
};
#[cfg(feature = "tart")]
pub use tart::{
    build_vibe_base as build_tart_vibe_base,
    ensure_configured_vibe_base as ensure_configured_tart_vibe_base, PreparedTartBase,
    TartBaseSource,
};
pub use vm_core::error::{Result as VmResult, VmError};

mod capabilities;
mod context;
#[cfg(any(feature = "docker", feature = "tart"))]
mod guest_cache;
mod instance;
#[cfg(any(feature = "docker", feature = "tart"))]
mod project_plan;
#[cfg(any(feature = "docker", feature = "tart"))]
mod resource_limits;
#[cfg(any(feature = "docker", feature = "tart"))]
mod resources;
#[cfg(any(feature = "docker", feature = "tart"))]
mod security;
#[cfg(any(feature = "docker", feature = "tart"))]
mod shell_session;
#[cfg(any(feature = "docker", feature = "tart"))]
mod stable_name;
mod status;
#[cfg(feature = "tart")]
mod tart;
mod temp_models;

#[cfg(feature = "tart")]
pub(crate) use tart::base as tart_base;

#[cfg(feature = "docker")]
mod audio;
#[cfg(feature = "docker")]
mod preflight;
#[cfg(any(feature = "docker", feature = "tart"))]
mod user_home;

#[cfg(feature = "docker")]
mod container;

// When the `test-helpers` feature is enabled, include the mock provider.
#[cfg(feature = "test-helpers")]
mod mock;

#[cfg(all(test, any(feature = "docker", feature = "tart")))]
mod resources_tests;

pub use temp_models::{Mount, MountPermission, TempVmState};

/// Factory-owned aggregate over the provider capabilities used by the CLI.
pub trait Provider: CommandProvider + InstanceProvider + ProvisioningProvider {
    /// Get access to temp provider capabilities if supported
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        None
    }

    /// Get access to tunnel capabilities if supported.
    fn as_tunnel_provider(&self) -> Option<&dyn TunnelProvider> {
        None
    }

    /// Clone the provider into a new Box.
    fn clone_box(&self) -> Box<dyn Provider>;
}

/// Validate the configured provider through its owning backend.
pub fn validate_provider_environment(provider: &ProviderName) -> Result<()> {
    match provider {
        #[cfg(feature = "docker")]
        ProviderName::Docker | ProviderName::Podman => {
            container::ContainerEngine::detect(provider)?.validate()
        }
        #[cfg(feature = "tart")]
        ProviderName::Tart => tart::validate_environment(),
        _ => Err(VmError::Provider(format!(
            "Provider '{provider}' is not enabled in this build"
        ))),
    }
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
    #[cfg(feature = "docker")]
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
    #[cfg(feature = "docker")]
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
    #[cfg(feature = "docker")]
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
