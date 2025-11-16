//! VM listing command handlers
//!
//! This module provides functionality for listing VMs across all providers
//! with filtering and display options.

use tracing::{debug, info_span};

use crate::error::VmResult;
use vm_cli::msg;
use vm_core::vm_println;
use vm_messages::messages::MESSAGES;
use vm_provider::{InstanceInfo, Provider};

/// Handle VM listing with enhanced filtering options
pub fn handle_list_enhanced(
    _provider: Box<dyn Provider>,
    _all_providers: &bool,
    provider_filter: Option<&str>,
    _verbose: &bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "list");
    let _enter = span.enter();
    debug!(
        "Listing VMs with enhanced filtering - provider_filter: {:?}",
        provider_filter
    );

    // Get all instances from all providers (or filtered)
    let all_instances = if let Some(provider_name) = provider_filter {
        get_instances_from_provider(provider_name)?
    } else {
        get_all_instances()?
    };

    if all_instances.is_empty() {
        if let Some(provider_name) = provider_filter {
            vm_println!(
                "{}",
                msg!(MESSAGES.vm_list_empty_provider, provider = provider_name)
            );
        } else {
            vm_println!("{}", MESSAGES.vm_list_empty);
        }
        return Ok(());
    }

    // Rich dashboard table (always displayed)
    vm_println!("{}", MESSAGES.vm_list_table_header);
    vm_println!("{}", MESSAGES.vm_list_table_separator);

    // Sort instances by provider then name for consistent output
    let mut sorted_instances = all_instances;
    sorted_instances.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));

    for instance in sorted_instances {
        vm_println!(
            "{:<20} {:<10} {:<12} {:<20} {:<10} {:<15}",
            truncate_string(&instance.name, 20),
            instance.provider,
            format_status(&instance.status),
            truncate_string(&instance.id, 20),
            format_uptime(&instance.uptime),
            instance.project.as_deref().unwrap_or("--")
        );
    }

    Ok(())
}

/// Legacy handle_list for backward compatibility
#[allow(dead_code)]
pub fn handle_list(provider: Box<dyn Provider>) -> VmResult<()> {
    handle_list_enhanced(provider, &true, None, &false)
}

// Helper function to get instances from all available providers
pub(super) fn get_all_instances() -> VmResult<Vec<InstanceInfo>> {
    use vm_config::config::VmConfig;
    use vm_provider::get_provider;

    let mut all_instances = Vec::new();
    let providers = ["docker", "podman", "tart"];

    for provider_name in providers {
        // Try to create each provider
        let config = VmConfig {
            provider: Some(provider_name.to_string()),
            ..Default::default()
        };

        match get_provider(config) {
            Ok(provider) => {
                // Get instances from this provider
                match provider.list_instances() {
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
                        // Continue with other providers
                    }
                }
            }
            Err(e) => {
                debug!("Provider {} not available: {}", provider_name, e);
                // Continue with other providers - this is expected if they're not installed
            }
        }
    }

    Ok(all_instances)
}

// Helper function to get instances from a specific provider
pub(super) fn get_instances_from_provider(provider_name: &str) -> VmResult<Vec<InstanceInfo>> {
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

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn format_status(status: &str) -> String {
    // Normalize status strings across providers with icons
    let lower_status = status.to_lowercase();
    if lower_status.contains("running") || lower_status.contains("up") {
        "✅ Running".to_string()
    } else if lower_status.contains("stopped")
        || lower_status.contains("exited")
        || lower_status.contains("poweroff")
    {
        "🔴 Stopped".to_string()
    } else if lower_status.contains("paused") {
        "⏸️  Paused".to_string()
    } else {
        format!("❓ {status}")
    }
}

fn format_uptime(uptime: &Option<String>) -> String {
    match uptime {
        Some(time) => time.clone(),
        None => "--".to_string(),
    }
}
