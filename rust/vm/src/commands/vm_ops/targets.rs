//! Shared target resolution helpers for cross-provider operations

use tracing::debug;

use crate::error::VmResult;
use vm_core::error::VmError;
use vm_provider::InstanceInfo;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstanceStateFilter {
    Running,
    Stopped,
}

#[derive(Clone, Copy, Debug)]
pub struct TargetQuery<'a> {
    pub provider: Option<&'a str>,
    pub pattern: Option<&'a str>,
    pub state: InstanceStateFilter,
}

/// Resolve instances across providers using one explicit query model.
pub fn resolve_targets(query: TargetQuery<'_>) -> VmResult<Vec<InstanceInfo>> {
    let instances = if let Some(provider_name) = query.provider {
        get_instances_from_provider(provider_name)?
    } else {
        get_all_instances()?
    };

    let mut filtered: Vec<InstanceInfo> = if let Some(pattern_str) = query.pattern {
        instances
            .into_iter()
            .filter(|instance| match_pattern(&instance.name, pattern_str))
            .collect()
    } else {
        instances
    };

    match query.state {
        InstanceStateFilter::Running => {
            filtered.retain(|instance| is_running_status(&instance.status));
        }
        InstanceStateFilter::Stopped => {
            filtered.retain(|instance| !is_running_status(&instance.status));
        }
    }

    Ok(filtered)
}

pub fn is_running_status(status: &str) -> bool {
    let lower = status.to_lowercase();
    lower.contains("running") || lower.contains("up")
}

pub fn project_instance_matches(instance: &InstanceInfo, project_name: &str) -> bool {
    instance.project.as_deref() == Some(project_name)
        || instance.name == project_name
        || instance.name == format!("{project_name}-dev")
        || instance.name.starts_with(&format!("{project_name}-"))
}

/// Helper function to get instances from all available providers
pub fn get_all_instances() -> VmResult<Vec<InstanceInfo>> {
    use vm_config::config::VmConfig;
    use vm_provider::get_provider;

    let mut all_instances = Vec::new();
    let mut provider_errors = Vec::new();
    let providers = ["docker", "podman", "tart"];

    for provider_name in providers {
        let config = VmConfig {
            provider: Some(provider_name.to_string()),
            ..Default::default()
        };

        match get_provider(config) {
            Ok(provider) => match provider.list_instances() {
                Ok(instances) => {
                    debug!(
                        "Found {} instances from {} provider",
                        instances.len(),
                        provider_name
                    );
                    all_instances.extend(instances);
                }
                Err(e) => {
                    debug!(
                        "Failed to list instances from {} provider: {}",
                        provider_name, e
                    );
                    provider_errors.push(format!("{provider_name}: {e}"));
                }
            },
            Err(e) => {
                debug!("Provider {} not available: {}", provider_name, e);
            }
        }
    }

    if all_instances.is_empty() && !provider_errors.is_empty() {
        return Err(VmError::Internal(format!(
            "Failed to list VM instances from any provider:\n{}",
            provider_errors.join("\n")
        ))
        .into());
    }

    Ok(all_instances)
}

/// Helper function to get instances from a specific provider
pub fn get_instances_from_provider(provider_name: &str) -> VmResult<Vec<InstanceInfo>> {
    use vm_config::config::VmConfig;
    use vm_provider::get_provider;

    let config = VmConfig {
        provider: Some(provider_name.to_string()),
        ..Default::default()
    };

    match get_provider(config) {
        Ok(provider) => match provider.list_instances() {
            Ok(instances) => {
                debug!(
                    "Found {} instances from {} provider",
                    instances.len(),
                    provider_name
                );
                Ok(instances)
            }
            Err(e) => {
                debug!(
                    "Failed to list instances from {} provider: {}",
                    provider_name, e
                );
                Err(VmError::Internal(format!(
                    "Failed to list instances from provider '{}': {}",
                    provider_name, e
                ))
                .into())
            }
        },
        Err(e) => {
            debug!("Provider {} not available: {}", provider_name, e);
            Err(VmError::Internal(format!(
                "Provider '{}' is not available: {}",
                provider_name, e
            ))
            .into())
        }
    }
}

/// Simple pattern matching for instance names
pub fn match_pattern(name: &str, pattern: &str) -> bool {
    if pattern.contains('*') {
        if pattern == "*" {
            true
        } else if pattern.starts_with('*') && pattern.ends_with('*') {
            let middle = &pattern[1..pattern.len() - 1];
            name.contains(middle)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            name.ends_with(suffix)
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            name.starts_with(prefix)
        } else {
            name == pattern
        }
    } else {
        name == pattern
    }
}
