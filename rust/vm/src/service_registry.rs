//! Service registry for managing service configurations and discovery
//!
//! This module centralizes service definitions, ports, health endpoints,
//! and provides a unified interface for service discovery and configuration.

use std::collections::HashMap;

use anyhow::Result;

/// Definition of a managed service
#[derive(Debug, Clone)]
pub struct ServiceDefinition {
    /// Service name (used as identifier)
    #[allow(dead_code)]
    pub name: String,
    /// Display name for user-facing messages
    pub display_name: String,
    /// Default port the service runs on
    pub port: u16,
    /// Health check endpoint relative to service URL
    #[allow(dead_code)]
    pub health_endpoint: String,
    /// Description of what the service provides
    #[allow(dead_code)]
    pub description: String,
    /// Whether the service supports graceful shutdown
    #[allow(dead_code)]
    pub supports_graceful_shutdown: bool,
}

impl ServiceDefinition {
    /// Get the full health check URL for this service
    #[allow(dead_code)]
    pub fn health_url(&self) -> String {
        format!("http://localhost:{}{}", self.port, self.health_endpoint)
    }

    /// Get the base URL for this service
    #[allow(dead_code)]
    pub fn base_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

/// Service registry providing centralized service definitions
pub struct ServiceRegistry {
    services: HashMap<String, ServiceDefinition>,
}

impl ServiceRegistry {
    /// Create a new service registry with default service definitions
    pub fn new() -> Self {
        let mut services = HashMap::new();

        // Auth Proxy Service
        services.insert(
            "auth_proxy".to_string(),
            ServiceDefinition {
                name: "auth_proxy".to_string(),
                display_name: "Auth Proxy".to_string(),
                port: 3090,
                health_endpoint: "/health".to_string(),
                description: "Centralized secrets management with encrypted storage".to_string(),
                supports_graceful_shutdown: true,
            },
        );

        // Docker Registry Service
        services.insert(
            "docker_registry".to_string(),
            ServiceDefinition {
                name: "docker_registry".to_string(),
                display_name: "Docker Registry".to_string(),
                port: 5000,
                health_endpoint: "/v2/".to_string(),
                description: "Docker image caching and pull-through proxy".to_string(),
                supports_graceful_shutdown: true,
            },
        );

        // Package Registry Service
        services.insert(
            "package_registry".to_string(),
            ServiceDefinition {
                name: "package_registry".to_string(),
                display_name: "Package Registry".to_string(),
                port: 3080,
                health_endpoint: "/health".to_string(),
                description: "Private package registry for npm, pip, and cargo".to_string(),
                supports_graceful_shutdown: true,
            },
        );

        let mut registry = Self { services };

        // Load plugin services (non-fatal if plugins unavailable)
        if let Err(e) = registry.load_plugin_services() {
            eprintln!("Warning: Failed to load plugin services: {e}");
        }

        registry
    }

    /// Load services from plugins
    fn load_plugin_services(&mut self) -> Result<()> {
        let plugins = vm_plugin::discover_plugins()?;
        let service_plugins = vm_plugin::get_service_plugins(&plugins);

        for plugin in service_plugins {
            // Load service content
            let content = match vm_plugin::load_service_content(plugin) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load service content from plugin {}: {}",
                        plugin.info.name, e
                    );
                    continue;
                }
            };

            // Parse port from first port mapping (format: "host:container" or just "port")
            let port = if let Some(port_mapping) = content.ports.first() {
                let port_str = port_mapping.split(':').next().unwrap_or(port_mapping);
                port_str.parse::<u16>().unwrap_or(8000)
            } else {
                8000 // Default port if none specified
            };

            // Create service definition from plugin service
            let service_def = ServiceDefinition {
                name: plugin.info.name.clone(),
                display_name: plugin
                    .info
                    .description
                    .clone()
                    .unwrap_or_else(|| plugin.info.name.clone()),
                port,
                health_endpoint: content.health_check.unwrap_or_else(|| "/".to_string()), // Use health_check or default to "/"
                description: plugin
                    .info
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("Service from {} plugin", plugin.info.name)),
                supports_graceful_shutdown: true,
            };

            // Add to registry (plugin services don't override built-in ones)
            self.services
                .entry(plugin.info.name.clone())
                .or_insert(service_def);
        }

        Ok(())
    }

    /// Get service definition by name
    #[allow(dead_code)]
    pub fn get_service(&self, name: &str) -> Option<&ServiceDefinition> {
        self.services.get(name)
    }

    /// Get all service definitions
    #[allow(dead_code)]
    pub fn get_all_services(&self) -> &HashMap<String, ServiceDefinition> {
        &self.services
    }

    /// Get service names that should be enabled based on VM configuration
    #[allow(dead_code)]
    pub fn get_enabled_services(&self, _config: &vm_config::config::VmConfig) -> Vec<String> {
        // Global services (auth_proxy, docker_registry, package_registry) are now
        // configured in GlobalConfig and are not checked here.
        Vec::new()
    }

    /// Check if a service is defined in the registry
    #[allow(dead_code)]
    pub fn is_service_defined(&self, name: &str) -> bool {
        self.services.contains_key(name)
    }

    /// Get service port by name
    pub fn get_service_port(&self, name: &str) -> Option<u16> {
        self.services.get(name).map(|s| s.port)
    }

    /// Get service display name
    pub fn get_service_display_name(&self, name: &str) -> Option<&str> {
        self.services.get(name).map(|s| s.display_name.as_str())
    }

    /// Validate that all enabled services in config are supported
    #[allow(dead_code)]
    pub fn validate_config_services(&self, config: &vm_config::config::VmConfig) -> Result<()> {
        let enabled_services = self.get_enabled_services(config);

        for service_name in &enabled_services {
            if !self.is_service_defined(service_name) {
                return Err(anyhow::anyhow!(
                    "Unknown service '{service_name}' enabled in configuration"
                ));
            }
        }

        Ok(())
    }

    /// Get status icon for service state
    pub fn get_status_icon(&self, is_running: bool) -> &'static str {
        if is_running {
            "🟢"
        } else {
            "🔴"
        }
    }

    /// Format service status for display
    pub fn format_service_status(
        &self,
        name: &str,
        is_running: bool,
        reference_count: u32,
    ) -> String {
        let icon = self.get_status_icon(is_running);
        let display_name = self.get_service_display_name(name).unwrap_or(name);
        let port = self.get_service_port(name).unwrap_or(0);

        if is_running {
            format!(
                "  {}: {} {} (port {}, {} VMs)",
                display_name, icon, "Running", port, reference_count
            )
        } else {
            format!("  {}: {} {} (port {})", display_name, icon, "Stopped", port)
        }
    }
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global service registry instance
static GLOBAL_SERVICE_REGISTRY: std::sync::OnceLock<ServiceRegistry> = std::sync::OnceLock::new();

/// Get the global service registry instance
pub fn get_service_registry() -> &'static ServiceRegistry {
    GLOBAL_SERVICE_REGISTRY.get_or_init(ServiceRegistry::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::VmConfig;

    #[test]
    fn test_service_registry_creation() {
        let registry = ServiceRegistry::new();

        assert!(registry.is_service_defined("auth_proxy"));
        assert!(registry.is_service_defined("docker_registry"));
        assert!(registry.is_service_defined("package_registry"));
        assert!(!registry.is_service_defined("unknown_service"));
    }

    #[test]
    fn test_service_ports() {
        let registry = ServiceRegistry::new();

        assert_eq!(registry.get_service_port("auth_proxy"), Some(3090));
        assert_eq!(registry.get_service_port("docker_registry"), Some(5000));
        assert_eq!(registry.get_service_port("package_registry"), Some(3080));
        assert_eq!(registry.get_service_port("unknown"), None);
    }

    #[test]
    fn test_enabled_services_from_config() {
        let registry = ServiceRegistry::new();

        // Global services are no longer configured per-VM, so this test
        // now verifies that no services are returned for VM-specific config
        let config = VmConfig::default();

        let enabled = registry.get_enabled_services(&config);
        assert_eq!(enabled.len(), 0); // No VM-specific global services

        // Test with default config (no longer uses deprecated fields)
        let config = VmConfig {
            ..Default::default()
        };

        let enabled = registry.get_enabled_services(&config);
        assert_eq!(enabled.len(), 0); // No global services from VM config
    }

    #[test]
    fn test_service_urls() {
        let registry = ServiceRegistry::new();
        let auth_service = registry.get_service("auth_proxy").unwrap();

        assert_eq!(auth_service.health_url(), "http://localhost:3090/health");
        assert_eq!(auth_service.base_url(), "http://localhost:3090");
    }

    #[test]
    fn test_status_formatting() {
        let registry = ServiceRegistry::new();

        let running_status = registry.format_service_status("auth_proxy", true, 2);
        assert!(running_status.contains("🟢"));
        assert!(running_status.contains("Running"));
        assert!(running_status.contains("2 VMs"));

        let stopped_status = registry.format_service_status("auth_proxy", false, 0);
        assert!(stopped_status.contains("🔴"));
        assert!(stopped_status.contains("Stopped"));
    }
}
