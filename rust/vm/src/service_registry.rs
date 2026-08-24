//! Service registry for managing service configurations and discovery
//!
//! This module centralizes service definitions, ports, health endpoints,
//! and provides a unified interface for service discovery and configuration.

use std::collections::HashMap;

use anyhow::Result;
use vm_core::vm_warning;

/// Definition of a managed service
#[derive(Debug, Clone)]
pub struct ServiceDefinition {
    /// Display name for user-facing messages
    pub display_name: String,
    /// Default port the service runs on
    pub port: u16,
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
                display_name: "Auth Proxy".to_string(),
                port: 3090,
            },
        );

        let mut registry = Self { services };

        // Load plugin services (non-fatal if plugins unavailable)
        if let Err(e) = registry.load_plugin_services() {
            vm_warning!("Failed to load plugin services: {e}");
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
                    vm_warning!(
                        "Failed to load service content from plugin {}: {}",
                        plugin.info.name,
                        e
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
                display_name: plugin
                    .info
                    .description
                    .clone()
                    .unwrap_or_else(|| plugin.info.name.clone()),
                port,
            };

            // Add to registry (plugin services don't override built-in ones)
            self.services
                .entry(plugin.info.name.clone())
                .or_insert(service_def);
        }

        Ok(())
    }

    /// Get service port by name
    pub fn get_service_port(&self, name: &str) -> Option<u16> {
        self.services.get(name).map(|s| s.port)
    }

    /// Get service display name
    pub fn get_service_display_name(&self, name: &str) -> Option<&str> {
        self.services.get(name).map(|s| s.display_name.as_str())
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
        let port = self.get_service_port(name).unwrap_or_default();

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

    #[test]
    fn test_service_ports() {
        let registry = ServiceRegistry::new();

        assert_eq!(registry.get_service_port("auth_proxy"), Some(3090));
        assert_eq!(registry.get_service_port("unknown"), None);
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
