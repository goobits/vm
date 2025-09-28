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
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::{debug, info, warn};

use vm_common::{vm_println, vm_success, vm_warning};
use vm_config::config::VmConfig;

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
#[derive(Debug)]
pub struct ServiceManager {
    /// Service state map protected by mutex for thread safety
    state: Arc<Mutex<HashMap<String, ServiceState>>>,
    /// Path to persistent state file
    state_file: PathBuf,
}

impl ServiceManager {
    /// Create a new ServiceManager instance
    pub fn new() -> Result<Self> {
        let vm_tool_dir = vm_common::user_paths::vm_state_dir()?;
        let state_file = vm_tool_dir.join("service_state.json");

        let manager = Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            state_file,
        };

        // Load existing state if available
        if let Err(e) = manager.load_state() {
            warn!("Failed to load service state: {}", e);
            debug!("Starting with clean service state");
        }

        Ok(manager)
    }

    /// Register services for a VM based on its configuration
    pub async fn register_vm_services(&self, vm_name: &str, config: &VmConfig) -> Result<()> {
        info!("Registering services for VM: {}", vm_name);

        let mut services_to_start = Vec::new();

        // Check which services are enabled in config
        if config.auth_proxy {
            services_to_start.push("auth_proxy");
        }
        if config.docker_registry {
            services_to_start.push("docker_registry");
        }
        if config.package_registry {
            services_to_start.push("package_registry");
        }

        // Update reference counts and track which services need starting
        let mut services_needing_start = Vec::new();
        {
            let mut state_guard = self.state.lock().unwrap();
            for service_name in &services_to_start {
                let service_state =
                    state_guard
                        .entry(service_name.to_string())
                        .or_insert_with(|| ServiceState {
                            port: self.get_service_port(service_name),
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
            if let Err(e) = self.start_service(&service_name).await {
                warn!("Failed to start service '{}': {}", service_name, e);
                // Don't fail VM creation if service startup fails
                vm_warning!("Service '{}' failed to start: {}", service_name, e);
            }
        }

        self.save_state()?;
        Ok(())
    }

    /// Unregister services for a VM
    pub async fn unregister_vm_services(&self, vm_name: &str) -> Result<()> {
        info!("Unregistering services for VM: {}", vm_name);

        let mut services_to_stop = Vec::new();

        // Update reference counts and identify services to stop
        {
            let mut state_guard = self.state.lock().unwrap();
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

                        // If reference count reaches zero, mark for stopping
                        if service_state.reference_count == 0 && service_state.is_running {
                            services_to_stop.push(service_name.clone());
                        }
                    }
                }
            }
        }

        // Stop services with zero references
        for service_name in services_to_stop {
            if let Err(e) = self.stop_service(&service_name).await {
                warn!("Failed to stop service '{}': {}", service_name, e);
            }
        }

        self.save_state()?;
        Ok(())
    }

    /// Ensure a service is running (for commands that need specific services)
    #[allow(dead_code)]
    pub async fn ensure_service_running(&self, service_name: &str) -> Result<bool> {
        let is_running = {
            let state_guard = self.state.lock().unwrap();
            state_guard
                .get(service_name)
                .map(|s| s.is_running)
                .unwrap_or(false)
        };

        if !is_running {
            debug!(
                "Service '{}' not running, checking actual status",
                service_name
            );
            // Check if service is actually running but not tracked
            if self.check_service_health(service_name).await {
                // Update state to reflect reality
                {
                    let mut state_guard = self.state.lock().unwrap();
                    #[allow(clippy::excessive_nesting)]
                    if let Some(service_state) = state_guard.get_mut(service_name) {
                        service_state.is_running = true;
                    }
                }
                self.save_state()?;
                return Ok(true);
            }
        }

        Ok(is_running)
    }

    /// Get service status information
    pub fn get_service_status(&self, service_name: &str) -> Option<ServiceState> {
        let state_guard = self.state.lock().unwrap();
        state_guard.get(service_name).cloned()
    }

    /// Get all service statuses
    #[allow(dead_code)]
    pub fn get_all_service_statuses(&self) -> HashMap<String, ServiceState> {
        let state_guard = self.state.lock().unwrap();
        state_guard.clone()
    }

    /// Start a service
    async fn start_service(&self, service_name: &str) -> Result<()> {
        info!("Starting service: {}", service_name);

        let port = self.get_service_port(service_name);

        match service_name {
            "auth_proxy" => {
                vm_println!("🚀 Starting auth proxy on port {}...", port);
                self.start_auth_proxy(port).await?;
            }
            "docker_registry" => {
                vm_println!("🚀 Starting Docker registry on port {}...", port);
                self.start_docker_registry().await?;
            }
            "package_registry" => {
                vm_println!("🚀 Starting package registry on port {}...", port);
                self.start_package_registry(port).await?;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown service: {}", service_name));
            }
        }

        // Update state
        {
            let mut state_guard = self.state.lock().unwrap();
            if let Some(service_state) = state_guard.get_mut(service_name) {
                service_state.is_running = true;
            }
        }

        // Verify service started
        for attempt in 1..=5 {
            sleep(Duration::from_millis(1000)).await;
            if self.check_service_health(service_name).await {
                vm_success!("Service '{}' started successfully", service_name);
                return Ok(());
            }
            debug!(
                "Service '{}' not ready, attempt {}/5",
                service_name, attempt
            );
        }

        Err(anyhow::anyhow!(
            "Service '{}' failed to start properly",
            service_name
        ))
    }

    /// Stop a service
    async fn stop_service(&self, service_name: &str) -> Result<()> {
        info!("Stopping service: {}", service_name);

        match service_name {
            "auth_proxy" => {
                vm_println!("🛑 Stopping auth proxy...");
                self.stop_auth_proxy().await?;
            }
            "docker_registry" => {
                vm_println!("🛑 Stopping Docker registry...");
                self.stop_docker_registry().await?;
            }
            "package_registry" => {
                vm_println!("🛑 Stopping package registry...");
                self.stop_package_registry().await?;
            }
            _ => {
                return Err(anyhow::anyhow!("Unknown service: {}", service_name));
            }
        }

        // Update state
        {
            let mut state_guard = self.state.lock().unwrap();
            if let Some(service_state) = state_guard.get_mut(service_name) {
                service_state.is_running = false;
                service_state.pid = None;
            }
        }

        vm_success!("Service '{}' stopped", service_name);
        Ok(())
    }

    /// Get the default port for a service
    fn get_service_port(&self, service_name: &str) -> u16 {
        match service_name {
            "auth_proxy" => 3090,
            "docker_registry" => 5000,
            "package_registry" => 3080,
            _ => 0,
        }
    }

    /// Check if a service is healthy
    async fn check_service_health(&self, service_name: &str) -> bool {
        let port = self.get_service_port(service_name);
        let endpoint = match service_name {
            "auth_proxy" => format!("http://localhost:{}/health", port),
            "docker_registry" => format!("http://localhost:{}/v2/", port),
            "package_registry" => format!("http://localhost:{}/health", port),
            _ => return false,
        };

        // Use reqwest to check health
        match reqwest::get(&endpoint).await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    /// Start auth proxy service
    async fn start_auth_proxy(&self, port: u16) -> Result<()> {
        use vm_auth_proxy;

        let data_dir = vm_auth_proxy::storage::get_auth_data_dir()
            .context("Failed to get auth data directory")?;

        tokio::spawn(async move {
            if let Err(e) =
                vm_auth_proxy::run_server_background("127.0.0.1".to_string(), port, data_dir).await
            {
                warn!("Auth proxy exited with error: {}", e);
            }
        });

        Ok(())
    }

    /// Stop auth proxy service
    async fn stop_auth_proxy(&self) -> Result<()> {
        // Implementation would send shutdown signal to auth proxy
        // For now, this is a placeholder
        debug!("Auth proxy stop requested");
        Ok(())
    }

    /// Start Docker registry service
    async fn start_docker_registry(&self) -> Result<()> {
        use vm_docker_registry;

        tokio::spawn(async move {
            if let Err(e) = vm_docker_registry::start_registry().await {
                warn!("Docker registry exited with error: {}", e);
            }
        });

        Ok(())
    }

    /// Stop Docker registry service
    async fn stop_docker_registry(&self) -> Result<()> {
        use vm_docker_registry;
        vm_docker_registry::stop_registry().await
    }

    /// Start package registry service
    async fn start_package_registry(&self, port: u16) -> Result<()> {
        use vm_package_server;

        let data_dir = std::env::current_dir()?.join(".vm-packages");

        tokio::spawn(async move {
            if let Err(e) = vm_package_server::server::run_server_background(
                "0.0.0.0".to_string(),
                port,
                data_dir,
            )
            .await
            {
                warn!("Package registry exited with error: {}", e);
            }
        });

        Ok(())
    }

    /// Stop package registry service
    async fn stop_package_registry(&self) -> Result<()> {
        // Implementation would send shutdown signal to package registry
        // For now, this is a placeholder
        debug!("Package registry stop requested");
        Ok(())
    }

    /// Save service state to disk
    fn save_state(&self) -> Result<()> {
        let state_guard = self.state.lock().unwrap();
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
            let mut state_guard = self.state.lock().unwrap();
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
