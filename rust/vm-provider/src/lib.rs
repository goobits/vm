//! VM provider abstraction library.
//!
//! This library provides a unified interface for working with different VM providers
//! such as Docker, Vagrant, and Tart. It defines core traits and factory functions
//! for provider instantiation and management.

// Standard library
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// External crates
use vm_core::error::Result;

// Internal imports
use vm_config::config::{BoxSpec, VmConfig};

// Re-export common types for convenience
pub use common::instance::{InstanceInfo, InstanceResolver};
pub use context::ProviderContext;
pub use vm_core::error::{Result as VmResult, VmError};

// Status report structures for enhanced dashboard
#[derive(Debug, Clone, Default)]
pub struct ResourceUsage {
    pub cpu_percent: Option<f64>,
    pub memory_used_mb: Option<u64>,
    pub memory_limit_mb: Option<u64>,
    pub disk_used_gb: Option<f64>,
    pub disk_total_gb: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub name: String,
    pub is_running: bool,
    pub port: Option<u16>,
    pub host_port: Option<u16>,
    pub metrics: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct VmStatusReport {
    pub name: String,
    pub provider: String,
    pub container_id: Option<String>,
    pub is_running: bool,
    pub uptime: Option<String>,
    pub resources: ResourceUsage,
    pub services: Vec<ServiceStatus>,
}

/// Request to create a VM snapshot
#[derive(Debug, Clone)]
pub struct SnapshotRequest {
    pub snapshot_name: String,
    pub description: Option<String>,
    pub quiesce: bool,
}

/// Request to restore a VM from snapshot
#[derive(Debug, Clone)]
pub struct SnapshotRestoreRequest {
    pub snapshot_name: String,
    pub snapshot_path: std::path::PathBuf,
    pub force: bool,
}

pub mod common;
pub mod context;
pub mod progress;
pub mod resources;
pub mod security;
pub mod temp_models;

pub mod audio;
pub mod preflight;

#[cfg(feature = "docker")]
pub mod docker;
#[cfg(feature = "docker")]
pub mod podman;
#[cfg(feature = "tart")]
pub mod tart;
#[cfg(feature = "vagrant")]
mod vagrant;

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

    /// Vagrant box from Vagrant Cloud (e.g., "ubuntu/focal64")
    VagrantBox(String),

    /// Tart OCI image (e.g., "ghcr.io/cirruslabs/ubuntu:latest")
    TartImage(String),

    /// Snapshot reference (universal across providers)
    Snapshot(String),
}

impl BoxConfig {
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

    /// Parse a BoxSpec for Vagrant provider
    ///
    /// # Detection Rules
    /// - Starts with `@` → Snapshot
    /// - Must contain `/` (user/box format) → Vagrant box
    /// - Build variant → Error (not supported)
    pub fn parse_for_vagrant(spec: &BoxSpec) -> Result<Self> {
        match spec {
            BoxSpec::String(s) => {
                // Snapshot
                if let Some(name) = s.strip_prefix('@') {
                    return Ok(BoxConfig::Snapshot(name.to_string()));
                }

                // Vagrant box (validate format: must be "user/boxname" with content on both sides)
                if !s.contains('/') {
                    return Err(VmError::Config(format!(
                        "Vagrant box must be in 'user/boxname' format, got: {}",
                        s
                    )));
                }

                // Ensure there's content before and after the first/last slash
                // This allows for nested paths like "company/team/box" while rejecting "/box" or "user/"
                if s.starts_with('/') || s.ends_with('/') {
                    return Err(VmError::Config(format!(
                        "Vagrant box must be in 'user/boxname' format with non-empty user and box name, got: {}",
                        s
                    )));
                }

                Ok(BoxConfig::VagrantBox(s.to_string()))
            }

            BoxSpec::Build { .. } => Err(VmError::Config(
                "Vagrant provider does not support Dockerfile builds".to_string(),
            )),
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

                // OCI image
                Ok(BoxConfig::TartImage(s.to_string()))
            }

            BoxSpec::Build { .. } => Err(VmError::Config(
                "Tart provider does not support Dockerfile builds".to_string(),
            )),
        }
    }
}

/// Trait for providers that support temporary VM mount updates
pub trait TempProvider {
    /// Update the mounts of a temporary VM by recreating the container
    fn update_mounts(&self, state: &TempVmState) -> Result<()>;

    /// Recreate a container with new mount configuration
    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()>;

    /// Check if a container is healthy and ready
    fn check_container_health(&self, container_name: &str) -> Result<bool>;

    /// Check if a container is currently running
    fn is_container_running(&self, container_name: &str) -> Result<bool>;
}

/// The core trait for all VM providers.
/// This defines the contract for creating, managing, and interacting with a VM.
pub trait Provider {
    /// Get the name of the provider (e.g., "docker", "vagrant").
    fn name(&self) -> &'static str;

    /// Create a new VM instance.
    /// This is the main provisioning step.
    fn create(&self) -> Result<()> {
        self.create_with_context(&ProviderContext::default())
    }

    /// Create a new VM instance with context.
    fn create_with_context(&self, context: &ProviderContext) -> Result<()>;

    /// Create a new VM instance with a specific name.
    /// This allows creating multiple instances of the same project.
    fn create_instance(&self, _instance_name: &str) -> Result<()> {
        // Default implementation falls back to regular create()
        // Providers can override this for true multi-instance support
        self.create()
    }

    /// Create a new VM instance with a specific name and context.
    fn create_instance_with_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        // Default implementation ignores context and calls old method for backward compatibility
        let _ = context;
        self.create_instance(instance_name)
    }

    /// Start an existing, stopped VM.
    fn start(&self, container: Option<&str>) -> Result<()>;

    /// Stop a running VM without destroying it.
    fn stop(&self, container: Option<&str>) -> Result<()>;

    /// Destroy a VM, removing all associated resources.
    fn destroy(&self, container: Option<&str>) -> Result<()>;

    /// Open an interactive shell (SSH) into the VM.
    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()>;

    /// Execute a command inside the VM.
    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()>;

    /// Get the logs of the VM.
    fn logs(&self, container: Option<&str>) -> Result<()>;

    /// Get the logs of the VM with extended options (follow, tail, service filtering).
    /// Providers that don't implement this will fall back to basic logs behavior.
    fn logs_extended(
        &self,
        container: Option<&str>,
        follow: bool,
        tail: usize,
        service: Option<&str>,
        _config: &VmConfig,
    ) -> Result<()> {
        // Default implementation: ignore new params and fall back to basic logs
        let _ = (follow, tail, service);
        self.logs(container)
    }

    /// Copy files to/from the VM.
    ///
    /// # Arguments
    /// * `source` - Source path (local file or container:path)
    /// * `destination` - Destination path (local file or container:path)
    /// * `container` - Optional container name for auto-detection
    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()>;

    /// Get a list of host paths mounted into the container.
    fn get_container_mounts(&self, _container_name: &str) -> Result<Vec<String>> {
        // Default implementation returns an empty vec for providers that don't support it
        Ok(Vec::new())
    }

    /// Get the status of the VM.
    fn status(&self, container: Option<&str>) -> Result<()>;

    /// Restart a VM (stop then start).
    fn restart(&self, container: Option<&str>) -> Result<()>;

    /// Start a VM with context (allows global config updates).
    /// This regenerates configuration files before starting.
    fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        // Default: fall back to regular start, ignore context
        let _ = context;
        self.start(container)
    }

    /// Restart a VM with context (allows global config updates).
    /// This regenerates configuration files before restarting.
    fn restart_with_context(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        // Default: fall back to regular restart, ignore context
        let _ = context;
        self.restart(container)
    }

    /// Re-run provisioning on existing VM.
    fn provision(&self, container: Option<&str>) -> Result<()>;

    /// List all VMs.
    fn list(&self) -> Result<()>;

    /// Force kill VM processes.
    /// If container is provided, kill that specific container. Otherwise, kill the project's container.
    fn kill(&self, container: Option<&str>) -> Result<()>;

    /// Get workspace directory.
    fn get_sync_directory(&self) -> String;

    /// Get access to temp provider capabilities if supported
    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        None
    }

    /// Resolve a partial instance name to a full instance name
    /// Returns the default instance if partial is None
    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        // Default implementation for backward compatibility
        match instance {
            Some(name) => Ok(name.to_string()),
            None => Ok("default".to_string()),
        }
    }

    /// List all instances managed by this provider
    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        // Default implementation calls existing list() method and returns empty
        self.list()?;
        Ok(vec![])
    }

    /// Check if this provider supports multiple instances
    fn supports_multi_instance(&self) -> bool {
        false // Default to single instance for backward compatibility
    }

    /// Get comprehensive status report for enhanced dashboard
    fn get_status_report(&self, _container: Option<&str>) -> Result<VmStatusReport> {
        Err(VmError::Provider(
            "Enhanced status not supported by this provider".to_string(),
        ))
    }

    /// Create a snapshot of the VM state
    ///
    /// For Docker: commits containers, saves images, backs up volumes
    /// For other providers: returns Unsupported error
    fn snapshot(&self, _request: &SnapshotRequest) -> Result<()> {
        Err(VmError::Provider(
            "Snapshots not supported by this provider".to_string(),
        ))
    }

    /// Restore a VM from a snapshot
    ///
    /// For Docker: loads images, restores volumes, recreates containers
    /// For other providers: returns Unsupported error
    fn restore_snapshot(&self, _request: &SnapshotRestoreRequest) -> Result<()> {
        Err(VmError::Provider(
            "Snapshot restore not supported by this provider".to_string(),
        ))
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
    let provider_name = config.provider.as_deref().unwrap_or("docker");

    #[cfg(feature = "test-helpers")]
    if provider_name == "mock" {
        return Ok(Box::new(mock::MockProvider::new(config)));
    }

    match provider_name {
        #[cfg(feature = "docker")]
        "docker" => Ok(Box::new(docker::DockerProvider::new(config)?)),
        #[cfg(feature = "docker")]
        "podman" => Ok(Box::new(podman::PodmanProvider::new(config)?)),
        #[cfg(feature = "vagrant")]
        "vagrant" => Ok(Box::new(vagrant::VagrantProvider::new(config)?)),
        #[cfg(feature = "tart")]
        "tart" => Ok(Box::new(tart::TartProvider::new(config)?)),
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
}
