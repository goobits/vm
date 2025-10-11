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

use vm_config::GlobalConfig;
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
#[derive(Debug, Clone)]
pub struct ServiceManager {
    /// Service state map protected by mutex for thread safety
    state: Arc<Mutex<HashMap<String, ServiceState>>>,
    /// Path to persistent state file
    state_file: PathBuf,
    /// Shutdown handles for services that support graceful shutdown
    shutdown_handles: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>>,
}

impl ServiceManager {
    /// Create a new ServiceManager instance
    pub fn new() -> Result<Self> {
        let state_file = vm_core::user_paths::services_state_path()?;

        let manager = Self {
            state: Arc::new(Mutex::new(HashMap::new())),
            state_file,
            shutdown_handles: Arc::new(Mutex::new(HashMap::new())),
        };

        // Load existing state if available
        if let Err(e) = manager.load_state() {
            warn!("Failed to load service state: {}", e);
            debug!("Starting with clean service state");
        }

        Ok(manager)
    }

    /// Register services for a VM based on global configuration
    pub async fn register_vm_services(
        &self,
        vm_name: &str,
        global_config: &GlobalConfig,
    ) -> Result<()> {
        info!("Registering services for VM: {}", vm_name);

        let mut services_to_start = Vec::new();

        // Check which services are enabled in global config
        if global_config.services.auth_proxy.enabled {
            services_to_start.push("auth_proxy");
        }
        if global_config.services.docker_registry.enabled {
            services_to_start.push("docker_registry");
        }
        if global_config.services.package_registry.enabled {
            services_to_start.push("package_registry");
        }
        if global_config.services.postgresql.enabled {
            services_to_start.push("postgresql");
        }
        if global_config.services.redis.enabled {
            services_to_start.push("redis");
        }
        if global_config.services.mongodb.enabled {
            services_to_start.push("mongodb");
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

        // Stop services with zero references in parallel for faster shutdown
        let stop_futures: Vec<_> = services_to_stop
            .into_iter()
            .map(|service_name| {
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
    async fn start_service(&self, service_name: &str, global_config: &GlobalConfig) -> Result<()> {
        info!("Starting service: {}", service_name);

        let port = self.get_service_port(service_name, global_config);

        match service_name {
            "auth_proxy" => {
                vm_println!("🚀 Starting auth proxy on port {}...", port);
                self.start_auth_proxy(port).await?;
            }
            "docker_registry" => {
                vm_println!("🚀 Starting Docker registry on port {}...", port);
                self.start_docker_registry(port).await?;
            }
            "package_registry" => {
                vm_println!("🚀 Starting package registry on port {}...", port);
                self.start_package_registry(port).await?;
            }
            "postgresql" => {
                vm_println!("🚀 Starting PostgreSQL on port {}...", port);
                self.start_postgres(global_config).await?;
            }
            "redis" => {
                vm_println!("🚀 Starting Redis on port {}...", port);
                self.start_redis(global_config).await?;
            }
            "mongodb" => {
                vm_println!("🚀 Starting MongoDB on port {}...", port);
                self.start_mongodb(global_config).await?;
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
            if self.check_service_health(service_name, global_config).await {
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
            "postgresql" => {
                vm_println!("🛑 Stopping PostgreSQL...");
                self.stop_postgres().await?;
            }
            "redis" => {
                vm_println!("🛑 Stopping Redis...");
                self.stop_redis().await?;
            }
            "mongodb" => {
                vm_println!("🛑 Stopping MongoDB...");
                self.stop_mongodb().await?;
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

    /// Get the port for a service from global configuration
    fn get_service_port(&self, service_name: &str, global_config: &GlobalConfig) -> u16 {
        match service_name {
            "auth_proxy" => global_config.services.auth_proxy.port,
            "docker_registry" => global_config.services.docker_registry.port,
            "package_registry" => global_config.services.package_registry.port,
            "postgresql" => global_config.services.postgresql.port,
            "redis" => global_config.services.redis.port,
            "mongodb" => global_config.services.mongodb.port,
            _ => 0,
        }
    }

    /// Check if a service is healthy
    async fn check_service_health(&self, service_name: &str, global_config: &GlobalConfig) -> bool {
        let port = self.get_service_port(service_name, global_config);
        let endpoint = match service_name {
            "auth_proxy" => format!("http://localhost:{}/health", port),
            "docker_registry" => format!("http://localhost:{}/v2/", port),
            "package_registry" => format!("http://localhost:{}/health", port),
            "postgresql" | "redis" | "mongodb" => {
                // For database services, a TCP connection is a reliable health check
                return tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                    .await
                    .is_ok();
            }
            _ => return false,
        };

        // Use reqwest to check health for HTTP-based services
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

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Store shutdown handle for later use
        {
            let mut handles = self.shutdown_handles.lock().unwrap();
            handles.insert("auth_proxy".to_string(), shutdown_tx);
        }

        // Spawn server with shutdown capability
        tokio::spawn(async move {
            if let Err(e) = vm_auth_proxy::run_server_with_shutdown(
                "127.0.0.1".to_string(),
                port,
                data_dir,
                Some(shutdown_rx),
            )
            .await
            {
                warn!("Auth proxy exited with error: {}", e);
            }
        });

        Ok(())
    }

    /// Stop auth proxy service
    async fn stop_auth_proxy(&self) -> Result<()> {
        debug!("Auth proxy stop requested");

        // Get shutdown handle
        let shutdown_tx = {
            let mut handles = self.shutdown_handles.lock().unwrap();
            handles.remove("auth_proxy")
        };

        if let Some(shutdown_tx) = shutdown_tx {
            // Send shutdown signal
            if shutdown_tx.send(()).is_err() {
                warn!(
                    "Failed to send shutdown signal to auth proxy (receiver may have been dropped)"
                );
            } else {
                info!("Shutdown signal sent to auth proxy");

                // Give the server a brief moment to shut down gracefully
                // Reduced from 1000ms to 200ms for faster stops
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        } else {
            warn!("No shutdown handle found for auth proxy - it may not be running or was started externally");
        }

        Ok(())
    }

    /// Start Docker registry service
    async fn start_docker_registry(&self, port: u16) -> Result<()> {
        use vm_docker_registry::{
            self, auto_manager::start_auto_manager, docker_config::configure_docker_daemon,
            server::start_registry_with_config, RegistryConfig,
        };

        // Create custom registry config with the specified port
        let config = RegistryConfig {
            registry_port: port,
            ..Default::default()
        };

        // Start the registry service with custom config
        tokio::spawn(async move {
            if let Err(e) = start_registry_with_config(&config).await {
                warn!("Docker registry exited with error: {}", e);
            }
        });

        // Wait a moment for the service to be available
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Configure the Docker daemon with the correct registry URL
        let registry_url = format!("http://127.0.0.1:{}", port);
        if let Err(e) = configure_docker_daemon(&registry_url).await {
            warn!("Failed to auto-configure Docker daemon: {}", e);
        }

        // Start the auto-manager background task
        if let Err(e) = start_auto_manager() {
            warn!("Failed to start registry auto-manager: {}", e);
        }

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

        let data_dir = vm_core::project::get_package_data_dir()?;

        // Create shutdown channel
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        // Store shutdown handle for later use
        {
            let mut handles = self.shutdown_handles.lock().unwrap();
            handles.insert("package_registry".to_string(), shutdown_tx);
        }

        // Spawn server with shutdown capability
        tokio::spawn(async move {
            if let Err(e) = vm_package_server::server::run_server_with_shutdown(
                "0.0.0.0".to_string(),
                port,
                data_dir,
                Some(shutdown_rx),
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
        debug!("Package registry stop requested");

        // Get shutdown handle
        let shutdown_tx = {
            let mut handles = self.shutdown_handles.lock().unwrap();
            handles.remove("package_registry")
        };

        if let Some(shutdown_tx) = shutdown_tx {
            // Send shutdown signal
            if shutdown_tx.send(()).is_err() {
                warn!("Failed to send shutdown signal to package registry (receiver may have been dropped)");
            } else {
                info!("Shutdown signal sent to package registry");

                // Give the server a brief moment to shut down gracefully
                // Reduced from 1000ms to 200ms for faster stops
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        } else {
            warn!("No shutdown handle found for package registry - it may not be running or was started externally");
        }

        Ok(())
    }

    /// Start PostgreSQL service
    async fn start_postgres(&self, global_config: &GlobalConfig) -> Result<()> {
        let settings = &global_config.services.postgresql;
        let container_name = "vm-postgres-global";

        // Expand tilde in data_dir
        let data_dir = shellexpand::tilde(&settings.data_dir).to_string();
        tokio::fs::create_dir_all(&data_dir).await?;

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(container_name)
            .arg("-p")
            .arg(format!("{}:5432", settings.port))
            .arg("-v")
            .arg(format!("{}:/var/lib/postgresql/data", data_dir))
            .arg("-e")
            .arg("POSTGRES_PASSWORD=postgres") // Simple default for local dev
            .arg(format!("postgres:{}", settings.version));

        let status = cmd.status().await?;
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to start PostgreSQL container"));
        }

        Ok(())
    }

    /// Stop PostgreSQL service
    async fn stop_postgres(&self) -> Result<()> {
        let container_name = "vm-postgres-global";

        // Stop the container
        let mut stop_cmd = tokio::process::Command::new("docker");
        stop_cmd.arg("stop").arg(container_name);
        if !stop_cmd.status().await?.success() {
            warn!("Failed to stop PostgreSQL container, it may not have been running.");
        }

        // Remove the container
        let mut rm_cmd = tokio::process::Command::new("docker");
        rm_cmd.arg("rm").arg(container_name);
        if !rm_cmd.status().await?.success() {
            warn!("Failed to remove PostgreSQL container.");
        }

        Ok(())
    }

    /// Start Redis service
    async fn start_redis(&self, global_config: &GlobalConfig) -> Result<()> {
        let settings = &global_config.services.redis;
        let container_name = "vm-redis-global";

        let data_dir = shellexpand::tilde(&settings.data_dir).to_string();
        tokio::fs::create_dir_all(&data_dir).await?;

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(container_name)
            .arg("-p")
            .arg(format!("{}:6379", settings.port))
            .arg("-v")
            .arg(format!("{}:/data", data_dir))
            .arg(format!("redis:{}", settings.version));

        let status = cmd.status().await?;
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to start Redis container"));
        }

        Ok(())
    }

    /// Stop Redis service
    async fn stop_redis(&self) -> Result<()> {
        let container_name = "vm-redis-global";

        let mut stop_cmd = tokio::process::Command::new("docker");
        stop_cmd.arg("stop").arg(container_name);
        if !stop_cmd.status().await?.success() {
            warn!("Failed to stop Redis container, it may not have been running.");
        }

        let mut rm_cmd = tokio::process::Command::new("docker");
        rm_cmd.arg("rm").arg(container_name);
        if !rm_cmd.status().await?.success() {
            warn!("Failed to remove Redis container.");
        }

        Ok(())
    }

    /// Start MongoDB service
    async fn start_mongodb(&self, global_config: &GlobalConfig) -> Result<()> {
        let settings = &global_config.services.mongodb;
        let container_name = "vm-mongodb-global";

        let data_dir = shellexpand::tilde(&settings.data_dir).to_string();
        tokio::fs::create_dir_all(&data_dir).await?;

        let mut cmd = tokio::process::Command::new("docker");
        cmd.arg("run")
            .arg("-d")
            .arg("--name")
            .arg(container_name)
            .arg("-p")
            .arg(format!("{}:27017", settings.port))
            .arg("-v")
            .arg(format!("{}:/data/db", data_dir))
            .arg(format!("mongo:{}", settings.version));

        let status = cmd.status().await?;
        if !status.success() {
            return Err(anyhow::anyhow!("Failed to start MongoDB container"));
        }

        Ok(())
    }

    /// Stop MongoDB service
    async fn stop_mongodb(&self) -> Result<()> {
        let container_name = "vm-mongodb-global";

        let mut stop_cmd = tokio::process::Command::new("docker");
        stop_cmd.arg("stop").arg(container_name);
        if !stop_cmd.status().await?.success() {
            warn!("Failed to stop MongoDB container, it may not have been running.");
        }

        let mut rm_cmd = tokio::process::Command::new("docker");
        rm_cmd.arg("rm").arg(container_name);
        if !rm_cmd.status().await?.success() {
            warn!("Failed to remove MongoDB container.");
        }

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
