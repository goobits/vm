//! Shared instance management types and utilities
//!
//! This module provides common types and functions for managing VM instances
//! across different providers. It defines a unified interface for instance
//! resolution and information handling.

use vm_cli::msg;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};
use vm_core::vm_error;
use vm_messages::messages::MESSAGES;

/// Information about a VM instance
#[derive(Debug, Clone)]
pub struct InstanceInfo {
    /// Human-readable instance name
    pub name: String,
    /// Provider-specific unique identifier
    pub id: String,
    /// Current status (running, stopped, etc.)
    pub status: String,
    /// Provider type (docker, tart, vagrant)
    pub provider: String,
    /// Associated project name, if any
    pub project: Option<String>,
    /// Uptime information (if available)
    pub uptime: Option<String>,
    /// Creation time (if available)
    pub created_at: Option<String>,
}

/// Common interface for instance resolution across providers
pub trait InstanceResolver {
    /// Resolve a partial instance name to a full instance name
    /// Returns the default instance if partial is None
    fn resolve_instance_name(&self, partial: Option<&str>) -> Result<String>;

    /// List all instances managed by this provider
    fn list_instances(&self) -> Result<Vec<InstanceInfo>>;

    /// Get the default instance name for this provider
    fn default_instance_name(&self) -> String;
}

/// Shared fuzzy matching logic for instance resolution
/// This is extracted from Docker's sophisticated resolution logic
pub fn fuzzy_match_instances(partial: &str, instances: &[InstanceInfo]) -> Result<String> {
    if instances.is_empty() {
        return Err(VmError::Internal(format!(
            "No instances found matching '{partial}'. Use 'vm list' to see available instances"
        )));
    }

    // First, try exact name match
    for instance in instances {
        if instance.name == partial {
            return Ok(instance.name.clone());
        }

        // Exact ID match (full or partial)
        if instance.id.starts_with(partial) {
            return Ok(instance.name.clone());
        }
    }

    // Second, try project name resolution (partial -> project-dev pattern)
    let candidate_name = format!("{partial}-dev");
    for instance in instances {
        if instance.name == candidate_name {
            return Ok(instance.name.clone());
        }
    }

    // Third, try fuzzy matching on instance names
    let mut matches = Vec::new();
    for instance in instances {
        if instance.name.contains(partial) {
            matches.push(instance.name.clone());
        }
    }

    match matches.len() {
        0 => Err(VmError::Internal(format!(
            "No instance found matching '{partial}'. Use 'vm list' to see available instances"
        ))),
        1 => Ok(matches[0].clone()),
        _ => {
            // Multiple matches - prefer exact project name match
            for name in &matches {
                if name == &format!("{partial}-dev") {
                    return Ok(name.clone());
                }
            }
            // Otherwise return first match but warn about ambiguity
            vm_error!("{}", MESSAGES.vm_ambiguous);
            for name in &matches {
                vm_error!("  • {}", name);
            }
            vm_error!("{}", msg!(MESSAGES.vm_using, name = &matches[0]));
            Ok(matches[0].clone())
        }
    }
}

/// Extract project name from config with fallback to default
pub fn extract_project_name(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|p| p.name.as_deref())
        .unwrap_or("vm-project")
}

/// Helper to create InstanceInfo for Docker containers
pub fn create_docker_instance_info(
    name: &str,
    id: &str,
    status: &str,
    created_at: Option<&str>,
    uptime: Option<&str>,
    project: Option<String>,
) -> InstanceInfo {
    // Use provided project name, or fallback to extracting from container name
    let project = project.or_else(|| {
        name.strip_suffix("-dev")
            .map(|project_part| project_part.to_string())
    });

    InstanceInfo {
        name: name.to_string(),
        id: id.to_string(),
        status: status.to_string(),
        provider: "docker".to_string(),
        project,
        uptime: uptime.map(|s| s.to_string()),
        created_at: created_at.map(|s| s.to_string()),
    }
}

/// Helper to create InstanceInfo for Tart VMs
pub fn create_tart_instance_info(
    name: &str,
    status: &str,
    created_at: Option<&str>,
    uptime: Option<&str>,
) -> InstanceInfo {
    // Extract project name from VM name (e.g., "myproject-dev" -> "myproject")
    let project = name
        .strip_suffix("-dev")
        .map(|project_part| project_part.to_string())
        .or_else(|| {
            name.strip_suffix("-staging")
                .map(|project_part| project_part.to_string())
        });

    InstanceInfo {
        name: name.to_string(),
        id: name.to_string(), // Tart uses VM name as ID
        status: status.to_string(),
        provider: "tart".to_string(),
        project,
        uptime: uptime.map(|s| s.to_string()),
        created_at: created_at.map(|s| s.to_string()),
    }
}

/// Helper to create InstanceInfo for Vagrant machines
pub fn create_vagrant_instance_info(
    name: &str,
    status: &str,
    project_name: &str,
    created_at: Option<&str>,
    uptime: Option<&str>,
) -> InstanceInfo {
    InstanceInfo {
        name: name.to_string(),
        id: format!("{project_name}:{name}"), // Combine project and machine name
        status: status.to_string(),
        provider: "vagrant".to_string(),
        project: Some(project_name.to_string()),
        uptime: uptime.map(|s| s.to_string()),
        created_at: created_at.map(|s| s.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match_exact_name() {
        let instances = vec![InstanceInfo {
            name: "myproject-dev".to_string(),
            id: "abc123".to_string(),
            status: "running".to_string(),
            provider: "docker".to_string(),
            project: Some("myproject".to_string()),
            uptime: None,
            created_at: None,
        }];

        let result = fuzzy_match_instances("myproject-dev", &instances)
            .expect("Should find exact match by name");
        assert_eq!(result, "myproject-dev");
    }

    #[test]
    fn test_fuzzy_match_partial_id() {
        let instances = vec![InstanceInfo {
            name: "myproject-dev".to_string(),
            id: "abc123def456".to_string(),
            status: "running".to_string(),
            provider: "docker".to_string(),
            project: Some("myproject".to_string()),
            uptime: None,
            created_at: None,
        }];

        let result =
            fuzzy_match_instances("abc123", &instances).expect("Should find match by partial ID");
        assert_eq!(result, "myproject-dev");
    }

    #[test]
    fn test_fuzzy_match_project_name() {
        let instances = vec![InstanceInfo {
            name: "myproject-dev".to_string(),
            id: "abc123".to_string(),
            status: "running".to_string(),
            provider: "docker".to_string(),
            project: Some("myproject".to_string()),
            uptime: None,
            created_at: None,
        }];

        let result = fuzzy_match_instances("myproject", &instances)
            .expect("Should find match by project name");
        assert_eq!(result, "myproject-dev");
    }

    #[test]
    fn test_fuzzy_match_no_matches() {
        let instances = vec![InstanceInfo {
            name: "otherproject-dev".to_string(),
            id: "xyz789".to_string(),
            status: "running".to_string(),
            provider: "docker".to_string(),
            project: Some("otherproject".to_string()),
            uptime: None,
            created_at: None,
        }];

        let result = fuzzy_match_instances("nonexistent", &instances);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No instance found"));
    }

    #[test]
    fn test_create_docker_instance_info() {
        let info =
            create_docker_instance_info("myproject-dev", "abc123", "running", None, None, None);
        assert_eq!(info.name, "myproject-dev");
        assert_eq!(info.id, "abc123");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "docker");
        assert_eq!(info.project, Some("myproject".to_string()));
        assert_eq!(info.uptime, None);
        assert_eq!(info.created_at, None);
    }

    #[test]
    fn test_create_tart_instance_info() {
        let info = create_tart_instance_info("myproject-staging", "running", None, None);
        assert_eq!(info.name, "myproject-staging");
        assert_eq!(info.id, "myproject-staging");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "tart");
        assert_eq!(info.project, Some("myproject".to_string()));
        assert_eq!(info.uptime, None);
        assert_eq!(info.created_at, None);
    }

    #[test]
    fn test_create_vagrant_instance_info() {
        let info = create_vagrant_instance_info("web", "running", "myproject", None, None);
        assert_eq!(info.name, "web");
        assert_eq!(info.id, "myproject:web");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "vagrant");
        assert_eq!(info.project, Some("myproject".to_string()));
        assert_eq!(info.uptime, None);
        assert_eq!(info.created_at, None);
    }

    #[test]
    fn test_create_docker_instance_info_with_metadata() {
        let info = create_docker_instance_info(
            "myproject-dev",
            "abc123",
            "running",
            Some("2023-01-01T00:00:00Z"),
            Some("2 hours ago"),
            None,
        );
        assert_eq!(info.name, "myproject-dev");
        assert_eq!(info.id, "abc123");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "docker");
        assert_eq!(info.project, Some("myproject".to_string()));
        assert_eq!(info.created_at, Some("2023-01-01T00:00:00Z".to_string()));
        assert_eq!(info.uptime, Some("2 hours ago".to_string()));
    }

    #[test]
    fn test_create_tart_instance_info_with_metadata() {
        let info = create_tart_instance_info(
            "myproject-staging",
            "running",
            Some("Created: 2023-01-01"),
            Some("running"),
        );
        assert_eq!(info.name, "myproject-staging");
        assert_eq!(info.id, "myproject-staging");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "tart");
        assert_eq!(info.project, Some("myproject".to_string()));
        assert_eq!(info.created_at, Some("Created: 2023-01-01".to_string()));
        assert_eq!(info.uptime, Some("running".to_string()));
    }
}
