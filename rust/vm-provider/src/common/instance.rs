//! Shared instance management types and utilities
//!
//! This module provides common types and functions for managing VM instances
//! across different providers. It defines a unified interface for instance
//! resolution and information handling.

use anyhow::Result;

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
        return Err(anyhow::anyhow!(
            "No instances found matching '{}'. Use 'vm list' to see available instances",
            partial
        ));
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
    let candidate_name = format!("{}-dev", partial);
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
        0 => Err(anyhow::anyhow!(
            "No instance found matching '{}'. Use 'vm list' to see available instances",
            partial
        )),
        1 => Ok(matches[0].clone()),
        _ => {
            // Multiple matches - prefer exact project name match
            for name in &matches {
                if name == &format!("{}-dev", partial) {
                    return Ok(name.clone());
                }
            }
            // Otherwise return first match but warn about ambiguity
            eprintln!(
                "Warning: Multiple instances match '{}': {}",
                partial,
                matches.join(", ")
            );
            eprintln!("Using: {}", matches[0]);
            Ok(matches[0].clone())
        }
    }
}

/// Helper to create InstanceInfo for Docker containers
pub fn create_docker_instance_info(name: &str, id: &str, status: &str) -> InstanceInfo {
    // Extract project name from container name (e.g., "myproject-dev" -> "myproject")
    let project = name
        .strip_suffix("-dev")
        .map(|project_part| project_part.to_string());

    InstanceInfo {
        name: name.to_string(),
        id: id.to_string(),
        status: status.to_string(),
        provider: "docker".to_string(),
        project,
        uptime: None,     // TODO: Extract from Docker status
        created_at: None, // TODO: Extract from Docker created_at
    }
}

/// Helper to create InstanceInfo for Tart VMs
pub fn create_tart_instance_info(name: &str, status: &str) -> InstanceInfo {
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
        uptime: None,     // TODO: Extract from Tart status
        created_at: None, // TODO: Extract from Tart created time
    }
}

/// Helper to create InstanceInfo for Vagrant machines
pub fn create_vagrant_instance_info(name: &str, status: &str, project_name: &str) -> InstanceInfo {
    InstanceInfo {
        name: name.to_string(),
        id: format!("{}:{}", project_name, name), // Combine project and machine name
        status: status.to_string(),
        provider: "vagrant".to_string(),
        project: Some(project_name.to_string()),
        uptime: None,     // TODO: Extract from Vagrant status
        created_at: None, // TODO: Extract from Vagrant created time
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

        let result = fuzzy_match_instances("myproject-dev", &instances).unwrap();
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

        let result = fuzzy_match_instances("abc123", &instances).unwrap();
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

        let result = fuzzy_match_instances("myproject", &instances).unwrap();
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
        let info = create_docker_instance_info("myproject-dev", "abc123", "running");
        assert_eq!(info.name, "myproject-dev");
        assert_eq!(info.id, "abc123");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "docker");
        assert_eq!(info.project, Some("myproject".to_string()));
    }

    #[test]
    fn test_create_tart_instance_info() {
        let info = create_tart_instance_info("myproject-staging", "running");
        assert_eq!(info.name, "myproject-staging");
        assert_eq!(info.id, "myproject-staging");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "tart");
        assert_eq!(info.project, Some("myproject".to_string()));
    }

    #[test]
    fn test_create_vagrant_instance_info() {
        let info = create_vagrant_instance_info("web", "running", "myproject");
        assert_eq!(info.name, "web");
        assert_eq!(info.id, "myproject:web");
        assert_eq!(info.status, "running");
        assert_eq!(info.provider, "vagrant");
        assert_eq!(info.project, Some("myproject".to_string()));
    }
}
