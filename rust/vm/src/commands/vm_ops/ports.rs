//! Port discovery command handler
//!
//! This module provides functionality to discover and display ports
//! that are exposed from the container.

use std::collections::BTreeMap;
use tracing::debug;

use crate::error::VmResult;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::vm_println;
use vm_provider::Provider;

#[derive(Debug)]
struct PortInfo {
    container_port: u16,
    host_port: Option<u16>,
    service_name: String,
    is_running: bool,
}

/// Handle ports command
///
/// Displays ports that are exposed from the container
/// along with their host port mappings and associated services.
pub fn handle_ports(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    _global_config: GlobalConfig,
) -> VmResult<()> {
    // Get VM name from config
    let vm_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .map(|s| s.as_str())
        .unwrap_or("vm-project");

    debug!("Port discovery: vm_name='{}'", vm_name);

    vm_println!("🔍 Port Mappings\n");

    // Get status report which includes port information
    match provider.get_status_report(container) {
        Ok(report) => {
            if !report.is_running {
                vm_println!("❌ Container is not running");
                vm_println!("   Start it with: vm up\n");
                return Ok(());
            }

            // Extract port information from services and sort by port
            let mut port_infos: BTreeMap<u16, PortInfo> = BTreeMap::new();

            for service in &report.services {
                if let Some(container_port) = service.port {
                    port_infos.insert(
                        container_port,
                        PortInfo {
                            container_port,
                            host_port: service.host_port,
                            service_name: service.name.clone(),
                            is_running: service.is_running,
                        },
                    );
                }
            }

            // Add configured ports from vm.yaml that might not be in services
            if !config.ports.mappings.is_empty() {
                for mapping in &config.ports.mappings {
                    port_infos.entry(mapping.guest).or_insert_with(|| PortInfo {
                        container_port: mapping.guest,
                        host_port: Some(mapping.host),
                        service_name: "application".to_string(),
                        is_running: true, // Assume running if container is up
                    });
                }
            }

            if port_infos.is_empty() {
                vm_println!("   No exposed ports found");
                vm_println!("\n💡 Configure ports in vm.yaml:");
                vm_println!("   ports:");
                vm_println!("     mappings:");
                vm_println!("       - host: 3000");
                vm_println!("         guest: 3000\n");
                return Ok(());
            }

            // Display results
            for (_, info) in port_infos {
                let status_icon = if info.is_running { "🟢" } else { "🔴" };

                let host_mapping = if let Some(host_port) = info.host_port {
                    if host_port == info.container_port {
                        format!("localhost:{}", host_port)
                    } else {
                        format!("localhost:{}→{}", host_port, info.container_port)
                    }
                } else {
                    format!(":{} (not exposed to host)", info.container_port)
                };

                vm_println!("  {} {} - {}", status_icon, host_mapping, info.service_name);
            }

            vm_println!("");
            Ok(())
        }
        Err(e) => {
            debug!("Failed to get status report: {}", e);
            vm_println!("❌ Unable to get port information");
            vm_println!("   Error: {}\n", e);
            vm_println!("💡 Tip: Make sure the container is running with `vm status`\n");
            Ok(())
        }
    }
}
