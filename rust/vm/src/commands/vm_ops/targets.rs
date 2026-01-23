//! Shared target resolution helpers for cross-provider operations

use tracing::debug;

use crate::error::VmResult;
use vm_provider::InstanceInfo;

/// Resolve instances across providers with optional filtering.
pub fn resolve_targets(
    provider_filter: Option<&str>,
    pattern: Option<&str>,
    running: bool,
    stopped: bool,
) -> VmResult<Vec<InstanceInfo>> {
    let instances = if let Some(provider_name) = provider_filter {
        get_instances_from_provider(provider_name)?
    } else {
        get_all_instances()?
    };

    let mut filtered: Vec<InstanceInfo> = if let Some(pattern_str) = pattern {
        instances
            .into_iter()
            .filter(|instance| match_pattern(&instance.name, pattern_str))
            .collect()
    } else {
        instances
    };

    if running ^ stopped {
        filtered.retain(|instance| {
            let is_running = is_running_status(&instance.status);
            if running {
                is_running
            } else {
                !is_running
            }
        });
    }

    Ok(filtered)
}

pub fn is_running_status(status: &str) -> bool {
    let lower = status.to_lowercase();
    lower.contains("running") || lower.contains("up")
}

/// Helper function to get instances from all available providers
pub fn get_all_instances() -> VmResult<Vec<InstanceInfo>> {
    use vm_config::config::VmConfig;
    use vm_provider::get_provider;

    let mut all_instances = Vec::new();
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
                }
            },
            Err(e) => {
                debug!("Provider {} not available: {}", provider_name, e);
            }
        }
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
                Ok(Vec::new())
            }
        },
        Err(e) => {
            debug!("Provider {} not available: {}", provider_name, e);
            Ok(Vec::new())
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
