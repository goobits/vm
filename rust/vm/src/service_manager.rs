//! Service lifecycle management with VM reference counting
//!
//! This module provides automatic service lifecycle management tied to VM operations.
//! Services are auto-started when first VM needs them and auto-stopped when last VM stops.
//!
//! # Architecture
//!
//! The ServiceManager maintains a reference count for each service, tracking how many VMs
//! are currently using each service. When the reference count reaches zero, services are
//! automatically stopped. When a VM needs a service that isn't running, it's automatically
//! started.
//!
//! # State Persistence
//!
//! Service state is persisted to disk to survive CLI restarts and system reboots.
//! This ensures reference counting remains accurate across sessions.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use futures::future;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use crate::error::VmError;
use crate::services::{
    auth_proxy::AuthProxyService, docker_registry::DockerRegistryService, mongodb::MongodbService,
    mysql::MysqlService, package_registry::PackageRegistryService, postgresql::PostgresqlService,
    redis::RedisService, ManagedService,
};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_println, vm_success, vm_warning};

/// Represents the current state of a managed service
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServiceState {
    /// Number of VMs currently using this service
    pub reference_count: u32,
    /// Whether the service is currently running
    pub is_running: bool,
    /// Port the service is running on
    pub port: u16,
    /// Process ID if available
    pub pid: Option<u32>,
    /// List of VMs currently using this service
    pub registered_vms: Vec<String>,
}

/// Central service lifecycle manager with reference counting
#[derive(Clone)]
pub struct ServiceManager {
    /// Service state map protected by mutex for thread safety
    state: Arc<Mutex<HashMap<String, ServiceState>>>,
    /// Path to persistent state file
    state_file: PathBuf,
    /// Shutdown handles for services that support graceful shutdown
    #[allow(dead_code)]
    shutdown_handles: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
    /// Service implementations
    services: Arc<Mutex<HashMap<String, Arc<dyn ManagedService>>>>,
}

impl ServiceManager {
    /// Create a new ServiceManager instance
    pub fn new() -> Result<Self> {
        let state_file = vm_core::user_paths::services_state_path()?;
        let shutdown_handles = Arc::new(Mutex::new(HashMap::new()));

        // Initialize all service implementations
        let mut services: HashMap<String, Arc<dyn ManagedService>> = HashMap::new();
        services.insert(
            "auth_proxy".to_string(),
            Arc::new(AuthProxyService::new(shutdown_handles.clone())),
        );
        services.insert(
            "docker_registry".to_string(),
            Arc::new(DockerRegistryService),
        );
        services.insert(
            "package_registry".to_string(),
            Arc::new(PackageRegistryService::new(shutdown_handles.clone())),
        );
        services.insert("postgresql".to_string(), Arc::new(PostgresqlService));
        services.insert("redis".to_string(), Arc::new(RedisService));
        services.insert("mongodb".to_string(), Arc::new(MongodbService));
        services.insert("mysql".to_string(), Arc::new(MysqlService));

        let manager = Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            state_file,
            shutdown_handles,
            services: Arc::new(Mutex::new(services)),
        };

        // Load existing state if available
        if let Err(e) = manager.load_state() {
            warn!("Failed to load service state: {}", e);
            debug!("Starting with clean service state");
        }

        Ok(manager)
    }

    /// Register services for a VM based on vm.yaml and global configuration
    ///
    /// Services are enabled if EITHER:
    /// - The VM's vm.yaml requests them (vm_config.services)
    /// - The global config enables them for all VMs (global_config.services)
    ///
    /// vm.yaml takes precedence for service-specific settings (port, version, etc.)
    pub async fn register_vm_services(
        &self,
        vm_name: &str,
        vm_config: &VmConfig,
        global_config: &GlobalConfig,
    ) -> Result<()> {
        info!("Registering services for VM: {}", vm_name);

        let mut services_to_start = Vec::new();

        // Helper to check if a service is enabled in vm.yaml OR global config
        let is_service_enabled = |service_name: &str| -> bool {
            // Check vm.yaml first (takes precedence)
            if vm_config
                .services
                .get(service_name)
                .is_some_and(|s| s.enabled)
            {
                return true;
            }
            // Fall back to global config
            match service_name {
                "postgresql" => global_config.services.postgresql.enabled,
                "redis" => global_config.services.redis.enabled,
                "mongodb" => global_config.services.mongodb.enabled,
                "mysql" => global_config.services.mysql.enabled,
                "auth_proxy" => global_config.services.auth_proxy.enabled,
                "docker_registry" => global_config.services.docker_registry.enabled,
                "package_registry" => global_config.services.package_registry.enabled,
                _ => false,
            }
        };

        // Check which services should be started (vm.yaml OR global config)
        if is_service_enabled("auth_proxy") {
            services_to_start.push("auth_proxy");
        }
        if is_service_enabled("docker_registry") {
            services_to_start.push("docker_registry");
        }
        if is_service_enabled("package_registry") {
            services_to_start.push("package_registry");
        }
        if is_service_enabled("postgresql") {
            services_to_start.push("postgresql");
        }
        if is_service_enabled("redis") {
            services_to_start.push("redis");
        }
        if is_service_enabled("mongodb") {
            services_to_start.push("mongodb");
        }
        if is_service_enabled("mysql") {
            services_to_start.push("mysql");
        }

        // Update reference counts and track which services need starting
        let mut services_needing_start = Vec::new();
        {
            let mut state_guard = self.state.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "State mutex was poisoned",
                )
            })?;
            for service_name in &services_to_start {
                let service_state =
                    state_guard
                        .entry(service_name.to_string())
                        .or_insert_with(|| ServiceState {
                            port: self.get_service_port(service_name, global_config),
                            ..Default::default()
                        });

                // Add VM to registered list if not already present
                if !service_state.registered_vms.contains(&vm_name.to_string()) {
                    service_state.registered_vms.push(vm_name.to_string());
                    service_state.reference_count += 1;

                    // If this is the first reference and service isn't running, mark for start
                    #[allow(clippy::excessive_nesting)]
                    if service_state.reference_count == 1 && !service_state.is_running {
                        services_needing_start.push(service_name.to_string());
                    }

                    info!(
                        "VM '{}' registered for service '{}' (ref count: {})",
                        vm_name, service_name, service_state.reference_count
                    );
                }
            }
        }

        // Start services that need starting
        for service_name in services_needing_start {
            if let Err(e) = self.start_service(&service_name, global_config).await {
                warn!("Failed to start service '{}': {}", service_name, e);
                // Don't fail VM creation if service startup fails
                vm_warning!("Service '{}' failed to start: {}", service_name, e);
            }
        }

        self.save_state()?;
        Ok(())
    }

    /// Unregister services for a VM
    pub async fn unregister_vm_services(
        &self,
        vm_name: &str,
        _global_config: &GlobalConfig,
    ) -> Result<()> {
        info!("Unregistering services for VM: {}", vm_name);

        let mut services_to_stop = Vec::new();

        // Update reference counts and identify services to stop
        {
            let mut state_guard = self.state.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "State mutex was poisoned",
                )
            })?;
            let service_names: Vec<String> = state_guard.keys().cloned().collect();

            for service_name in service_names {
                if let Some(service_state) = state_guard.get_mut(&service_name) {
                    // Remove VM from registered list
                    #[allow(clippy::excessive_nesting)]
                    if let Some(pos) = service_state
                        .registered_vms
                        .iter()
                        .position(|vm| vm == vm_name)
                    {
                        service_state.registered_vms.remove(pos);
                        service_state.reference_count =
                            service_state.reference_count.saturating_sub(1);

                        info!(
                            "VM '{}' unregistered from service '{}' (ref count: {})",
                            vm_name, service_name, service_state.reference_count
                        );

                        // If reference count reaches zero, perform cleanup and mark for stopping
                        if service_state.reference_count == 0 && service_state.is_running {
                            services_to_stop.push(service_name.clone());
                        }
                    }
                }
            }
        }

        // Stop services with zero references in parallel for faster shutdown
        let stop_futures: Vec<_> = services_to_stop
            .into_iter()
            .map(|service_name: String| {
                let self_clone = self.clone();
                async move {
                    if let Err(e) = self_clone.stop_service(&service_name).await {
                        warn!("Failed to stop service '{}': {}", service_name, e);
                    }
                }
            })
            .collect();

        // Wait for all services to stop
        future::join_all(stop_futures).await;

        self.save_state()?;
        Ok(())
    }

    /// Get service status information
    pub fn get_service_status(&self, service_name: &str) -> Option<ServiceState> {
        self.state
            .lock()
            .ok()
            .and_then(|guard| guard.get(service_name).cloned())
    }

    /// Get all service statuses
    #[allow(dead_code)]
    pub fn get_all_service_statuses(&self) -> HashMap<String, ServiceState> {
        self.state
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Start a service
    async fn start_service(&self, service_name: &str, global_config: &GlobalConfig) -> Result<()> {
        info!("Starting service: {}", service_name);

        // Get the service implementation (clone Arc before dropping lock)
        let service_impl = {
            let services_guard = self.services.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "Services mutex was poisoned",
                )
            })?;
            services_guard
                .get(service_name)
                .ok_or_else(|| anyhow::anyhow!("Unknown service: {service_name}"))?
                .clone()
        };

        let port = self.get_service_port(service_name, global_config);
        vm_println!("🚀 Starting {} on port {}...", service_impl.name(), port);

        // Use trait dispatch to start the service (lock is dropped)
        service_impl.start(global_config).await?;

        // Update state
        {
            let mut state_guard = self.state.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "State mutex was poisoned",
                )
            })?;
            if let Some(service_state) = state_guard.get_mut(service_name) {
                service_state.is_running = true;
            }
        }

        // Verify service started
        for attempt in 1..=10 {
            sleep(Duration::from_millis(2000)).await;
            if self.check_service_health(service_name, global_config).await {
                vm_success!("Service '{}' started successfully", service_name);
                return Ok(());
            }
            debug!(
                "Service '{}' not ready, attempt {}/10",
                service_name, attempt
            );
        }

        Err(anyhow::anyhow!(
            "Service '{service_name}' failed to start properly"
        ))
    }

    /// Stop a service
    async fn stop_service(&self, service_name: &str) -> Result<()> {
        info!("Stopping service: {}", service_name);

        // Get the service implementation (clone Arc before dropping lock)
        let service_impl = {
            let services_guard = self.services.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "Services mutex was poisoned",
                )
            })?;
            services_guard
                .get(service_name)
                .ok_or_else(|| anyhow::anyhow!("Unknown service: {service_name}"))?
                .clone()
        };

        vm_println!("🛑 Stopping {}...", service_impl.name());

        // Use trait dispatch to stop the service (lock is dropped)
        service_impl.stop().await?;

        // Update state
        {
            let mut state_guard = self.state.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "State mutex was poisoned",
                )
            })?;
            if let Some(service_state) = state_guard.get_mut(service_name) {
                service_state.is_running = false;
                service_state.pid = None;
            }
        }

        vm_success!("Service '{}' stopped", service_name);
        Ok(())
    }

    /// Get the port for a service from global configuration
    fn get_service_port(&self, service_name: &str, global_config: &GlobalConfig) -> u16 {
        self.services
            .lock()
            .ok()
            .and_then(|guard| guard.get(service_name).map(|s| s.get_port(global_config)))
            .unwrap_or(0)
    }

    /// Check if a service is healthy
    async fn check_service_health(&self, service_name: &str, global_config: &GlobalConfig) -> bool {
        // Get the service implementation (clone Arc before dropping lock)
        let service_impl = {
            let services_guard = match self.services.lock() {
                Ok(guard) => guard,
                Err(_) => return false,
            };
            match services_guard.get(service_name) {
                Some(service) => service.clone(),
                None => return false,
            }
        };

        // Use trait dispatch to check service health (lock is dropped)
        service_impl.check_health(global_config).await
    }

    /// Save service state to disk
    fn save_state(&self) -> Result<()> {
        let state_guard = self.state.lock().map_err(|e| {
            VmError::general(
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                "State mutex was poisoned",
            )
        })?;
        let json = serde_json::to_string_pretty(&*state_guard)
            .context("Failed to serialize service state")?;

        if let Some(parent) = self.state_file.parent() {
            std::fs::create_dir_all(parent).context("Failed to create service state directory")?;
        }

        std::fs::write(&self.state_file, json).context("Failed to write service state file")?;

        debug!("Service state saved to {:?}", self.state_file);
        Ok(())
    }

    /// Load service state from disk
    fn load_state(&self) -> Result<()> {
        if !self.state_file.exists() {
            debug!("No existing service state file found");
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.state_file)
            .context("Failed to read service state file")?;

        let loaded_state: HashMap<String, ServiceState> =
            serde_json::from_str(&content).context("Failed to parse service state file")?;

        {
            let mut state_guard = self.state.lock().map_err(|e| {
                VmError::general(
                    std::io::Error::new(std::io::ErrorKind::Other, e.to_string()),
                    "State mutex was poisoned",
                )
            })?;
            *state_guard = loaded_state;
        }

        info!("Service state loaded from {:?}", self.state_file);
        Ok(())
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new().expect("Failed to create ServiceManager")
    }
}

/// Global service manager instance
static GLOBAL_SERVICE_MANAGER: std::sync::OnceLock<ServiceManager> = std::sync::OnceLock::new();

/// Get the global service manager instance
pub fn get_service_manager() -> &'static ServiceManager {
    GLOBAL_SERVICE_MANAGER
        .get_or_init(|| ServiceManager::new().expect("Failed to initialize global service manager"))
}
