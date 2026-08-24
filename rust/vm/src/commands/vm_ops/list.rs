//! VM listing command handlers
//!
//! This module provides functionality for listing VMs across all providers
//! with filtering and display options.

use tracing::{debug, info_span};

use crate::commands::vm_ops::target::project_instance_matches;
use crate::commands::vm_ops::targets::{get_all_instances, get_instances_from_provider};
use crate::error::VmResult;
use vm_core::vm_println;
use vm_provider::{InstanceInfo, InstanceProvider};

/// Handle VM listing with enhanced filtering options
pub fn handle_list_enhanced(
    configured_provider: Option<&dyn InstanceProvider>,
    provider_filter: Option<&str>,
    project_filter: Option<&str>,
    raw: bool,
    default_name: Option<&str>,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "list");
    let _enter = span.enter();
    debug!(
        "Listing VMs with enhanced filtering - provider_filter: {:?}",
        provider_filter
    );

    // Use the loaded project provider when available so provider-specific
    // settings such as Tart's storage path remain in effect.
    let mut all_instances = load_instances(configured_provider, provider_filter)?;

    if let Some(project_name) = project_filter {
        all_instances.retain(|instance| project_instance_matches(instance, project_name));
    }

    if all_instances.is_empty() {
        if let Some(provider_name) = provider_filter {
            vm_println!("No environments found for provider '{provider_name}'");
        } else {
            vm_println!("No environments found");
        }
        return Ok(());
    }

    if raw {
        render_raw_instance_table(all_instances, default_name);
    } else {
        render_instance_table(all_instances, default_name);
    }

    Ok(())
}

fn load_instances(
    configured_provider: Option<&dyn InstanceProvider>,
    provider_filter: Option<&str>,
) -> VmResult<Vec<InstanceInfo>> {
    if let Some(provider) = configured_provider {
        return provider.list_instances().map_err(Into::into);
    }
    if let Some(provider_name) = provider_filter {
        return get_instances_from_provider(provider_name);
    }
    get_all_instances()
}

pub fn render_instance_table(instances: Vec<InstanceInfo>, default_name: Option<&str>) {
    vm_println!(
        "{:<20} {:<9} {:<12} {:<12} {:<10}",
        "ENVIRONMENT",
        "DEFAULT",
        "KIND",
        "STATUS",
        "UPTIME"
    );
    vm_println!("{}", "─".repeat(69));

    let mut sorted_instances = instances;
    sorted_instances.sort_by(|a, b| a.name.cmp(&b.name));

    for instance in sorted_instances {
        vm_println!(
            "{:<20} {:<9} {:<12} {:<12} {:<10}",
            truncate_string(&instance.name, 20),
            if default_name == Some(instance.name.as_str()) {
                "yes"
            } else {
                ""
            },
            format_kind(&instance),
            format_status(&instance.status),
            format_uptime(&instance.uptime)
        );
    }
}

fn render_raw_instance_table(instances: Vec<InstanceInfo>, default_name: Option<&str>) {
    vm_println!(
        "{:<20} {:<8} {:<10} {:<12} {:<20} {:<10} {:<15}",
        "ENVIRONMENT",
        "DEFAULT",
        "PROVIDER",
        "STATUS",
        "ID",
        "UPTIME",
        "PROJECT"
    );
    vm_println!("{}", "─".repeat(105));

    let mut sorted_instances = instances;
    sorted_instances.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));

    for instance in sorted_instances {
        vm_println!(
            "{:<20} {:<8} {:<10} {:<12} {:<20} {:<10} {:<15}",
            truncate_string(&instance.name, 20),
            if default_name == Some(instance.name.as_str()) {
                "yes"
            } else {
                ""
            },
            instance.provider,
            format_status(&instance.status),
            truncate_string(&instance.id, 20),
            format_uptime(&instance.uptime),
            instance.project.as_deref().unwrap_or("--")
        );
    }
}

fn format_kind(instance: &InstanceInfo) -> &'static str {
    match instance.provider.as_str() {
        "docker" | "podman" => "Container",
        "tart" if instance.name == "mac" || instance.name.ends_with("-mac") => "macOS",
        "tart" => "Linux",
        _ => "Unknown",
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!(
            "{}...",
            s.chars()
                .take(max_len.saturating_sub(3))
                .collect::<String>()
        )
    }
}

fn format_status(status: &str) -> String {
    // Normalize status strings across providers with icons
    let lower_status = status.to_lowercase();
    if lower_status.contains("running") || lower_status.contains("up") {
        "🟢 Running".to_string()
    } else if lower_status.contains("stopped")
        || lower_status.contains("exited")
        || lower_status.contains("poweroff")
    {
        "💤 Stopped".to_string()
    } else if lower_status.contains("paused") {
        "⏸️  Paused".to_string()
    } else {
        format!("❓ {status}")
    }
}

fn format_uptime(uptime: &Option<String>) -> String {
    match uptime {
        Some(time) => time.clone(),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::load_instances;
    use vm_provider::mock::MockProvider;

    #[test]
    fn project_listing_uses_the_configured_provider() {
        let provider = MockProvider;

        let instances = load_instances(Some(&provider), None).unwrap();

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "mock-vm");
    }
}
