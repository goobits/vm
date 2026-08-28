//! Provider-neutral instance metadata and resolution helpers.
//!
//! This module provides common types and functions for managing VM instances
//! across different providers. It defines a unified interface for instance
//! resolution and information handling.

#[cfg(feature = "tart")]
use vm_config::config::VmConfig;
#[cfg(any(feature = "docker", feature = "tart", test))]
use vm_core::error::{Result, VmError};

/// Information about a VM instance
#[derive(Debug, Clone)]
pub struct InstanceInfo {
    /// Human-readable instance name
    pub name: String,
    /// Provider-specific unique identifier
    pub id: String,
    /// Current status (running, stopped, etc.)
    pub status: String,
    /// Provider type (docker, tart, podman)
    pub provider: String,
    /// Associated project name, if any
    pub project: Option<String>,
    /// Uptime information (if available)
    pub uptime: Option<String>,
    /// Creation time (if available)
    pub created_at: Option<String>,
}

/// Shared fuzzy matching logic for instance resolution
/// This is extracted from Docker's sophisticated resolution logic
#[cfg(any(feature = "docker", feature = "tart", test))]
pub(crate) fn fuzzy_match_instances(partial: &str, instances: &[InstanceInfo]) -> Result<String> {
    if instances.is_empty() {
        return Err(VmError::NotFound(format!(
            "No instances found matching '{partial}'. Use 'vm list' to see available instances"
        )));
    }

    if let Some(instance) = instances.iter().find(|instance| instance.name == partial) {
        return Ok(instance.name.clone());
    }

    let id_matches: Vec<_> = instances
        .iter()
        .filter(|instance| instance.id.starts_with(partial))
        .collect();
    match id_matches.as_slice() {
        [instance] => return Ok(instance.name.clone()),
        [] => {}
        _ => {
            let mut names: Vec<_> = id_matches
                .iter()
                .map(|instance| instance.name.as_str())
                .collect();
            names.sort_unstable();
            return Err(VmError::Internal(format!(
                "Ambiguous instance ID '{partial}' matches: {}. Use an exact name or longer ID",
                names.join(", ")
            )));
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
        0 => Err(VmError::NotFound(format!(
            "No instance found matching '{partial}'. Use 'vm list' to see available instances"
        ))),
        1 => Ok(matches[0].clone()),
        _ => {
            matches.sort_unstable();
            Err(VmError::Internal(format!(
                "Ambiguous instance name '{partial}' matches: {}. Use an exact name",
                matches.join(", ")
            )))
        }
    }
}

/// Extract project name from config with fallback to default
#[cfg(feature = "tart")]
pub(crate) fn extract_project_name(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|p| p.name.as_deref())
        .unwrap_or("vm-project")
}

/// Helper to create instance information for Docker-compatible containers.
#[cfg(any(feature = "docker", test))]
pub(crate) fn create_container_instance_info(
    provider: &str,
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
        provider: provider.to_string(),
        project,
        uptime: uptime.map(|s| s.to_string()),
        created_at: created_at.map(|s| s.to_string()),
    }
}

/// Helper to create InstanceInfo for Tart VMs
#[cfg(any(feature = "tart", test))]
pub(crate) fn create_tart_instance_info(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn instance(name: &str, id: &str) -> InstanceInfo {
        InstanceInfo {
            name: name.to_string(),
            id: id.to_string(),
            status: "running".to_string(),
            provider: "docker".to_string(),
            project: None,
            uptime: None,
            created_at: None,
        }
    }

    fn fields(
        info: &InstanceInfo,
    ) -> (
        &str,
        &str,
        &str,
        &str,
        Option<&str>,
        Option<&str>,
        Option<&str>,
    ) {
        (
            &info.name,
            &info.id,
            &info.status,
            &info.provider,
            info.project.as_deref(),
            info.created_at.as_deref(),
            info.uptime.as_deref(),
        )
    }

    #[test]
    fn resolves_exact_name_partial_id_and_project_name() {
        let instances = [instance("myproject-dev", "abc123def456")];

        for partial in ["myproject-dev", "abc123", "myproject"] {
            assert_eq!(
                fuzzy_match_instances(partial, &instances).unwrap(),
                "myproject-dev"
            );
        }
    }

    #[test]
    fn missing_instance_returns_not_found() {
        let error = fuzzy_match_instances("nonexistent", &[instance("otherproject-dev", "xyz789")])
            .unwrap_err();
        assert!(matches!(error, VmError::NotFound(_)));
    }

    #[test]
    fn ambiguous_instance_names_are_sorted() {
        let instances = [instance("api-two", "id2"), instance("api-one", "id1")];

        let error = fuzzy_match_instances("api", &instances).unwrap_err();
        assert!(error.to_string().contains("Ambiguous instance name"));
        assert!(error.to_string().contains("api-one, api-two"));
    }

    #[test]
    fn creates_container_instance_metadata() {
        for (provider, created_at, uptime) in [
            ("docker", None, None),
            ("podman", Some("2023-01-01T00:00:00Z"), Some("2 hours ago")),
        ] {
            let info = create_container_instance_info(
                provider,
                "myproject-dev",
                "abc123",
                "running",
                created_at,
                uptime,
                None,
            );
            assert_eq!(
                fields(&info),
                (
                    "myproject-dev",
                    "abc123",
                    "running",
                    provider,
                    Some("myproject"),
                    created_at,
                    uptime,
                )
            );
        }
    }

    #[test]
    fn creates_tart_instance_metadata() {
        for (created_at, uptime) in [(None, None), (Some("Created: 2023-01-01"), Some("running"))] {
            let info =
                create_tart_instance_info("myproject-staging", "running", created_at, uptime);
            assert_eq!(
                fields(&info),
                (
                    "myproject-staging",
                    "myproject-staging",
                    "running",
                    "tart",
                    Some("myproject"),
                    created_at,
                    uptime,
                )
            );
        }
    }
}
