//! VM listing command handlers
//!
//! This module provides functionality for listing VMs across all providers
//! with filtering and display options.

use serde::Serialize;
use tracing::{debug, info_span};

use crate::commands::vm_ops::target::canonical_instance_name;
use crate::commands::vm_ops::targets::{get_all_instances, get_instances_from_provider};
use crate::error::VmResult;
use vm_config::config::VmConfig;
use vm_core::vm_println;
use vm_provider::{get_provider, InstanceInfo, InstanceProvider};

use super::fleet::{filter_project_instances, FleetProject};

#[derive(Serialize)]
pub struct ListOutput {
    pub count: usize,
    pub environments: Vec<ListEntry>,
}

#[derive(Serialize)]
pub struct ListEntry {
    name: String,
    provider: String,
    status: String,
    is_default: bool,
    project: Option<String>,
    uptime: Option<String>,
    id: Option<String>,
}

pub fn list_output(
    mut instances: Vec<InstanceInfo>,
    default_name: Option<&str>,
    raw: bool,
) -> ListOutput {
    instances.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));
    let environments = instances
        .into_iter()
        .map(|instance| ListEntry {
            is_default: default_name == Some(instance.name.as_str()),
            name: instance.name,
            provider: instance.provider,
            status: instance.status,
            project: instance.project,
            uptime: instance.uptime,
            id: raw.then_some(instance.id),
        })
        .collect::<Vec<_>>();
    ListOutput {
        count: environments.len(),
        environments,
    }
}

pub fn collect_declared_project_instances(
    config: &VmConfig,
) -> VmResult<(Vec<InstanceInfo>, Option<String>)> {
    let project = config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project");
    let mut providers = std::collections::BTreeMap::new();
    if let Some(provider) = &config.provider {
        providers.insert(provider.as_str().to_string(), config.clone());
    }
    for declaration in config.environments.values() {
        providers
            .entry(declaration.provider.as_str().to_string())
            .or_insert_with(|| declaration.apply_to(config));
    }
    let mut instances = Vec::new();
    let mut unavailable = std::collections::BTreeSet::new();
    for (provider_name, selected) in providers {
        match get_provider(selected).and_then(|provider| provider.list_instances()) {
            Ok(discovered) => instances.extend(discovered),
            Err(error) => {
                debug!(provider = %provider_name, %error, "Provider inventory unavailable");
                unavailable.insert(provider_name);
            }
        }
    }
    let mut instances = filter_project_instances(instances, &FleetProject::new(config.clone())?)?;
    for (name, declaration) in &config.environments {
        let runtime_name =
            canonical_instance_name(declaration.provider.as_str(), project, Some(name));
        if !instances.iter().any(|instance| {
            instance.provider == declaration.provider.as_str() && instance.name == runtime_name
        }) {
            instances.push(InstanceInfo {
                name: runtime_name,
                id: String::new(),
                status: if unavailable.contains(declaration.provider.as_str()) {
                    "unavailable".into()
                } else {
                    "declared".into()
                },
                provider: declaration.provider.as_str().into(),
                project: Some(project.into()),
                uptime: None,
                created_at: None,
            });
        }
    }
    let default_name = config
        .project
        .as_ref()
        .and_then(|project| project.default_environment.as_deref())
        .and_then(|name| {
            config.environments.get(name).map(|declaration| {
                canonical_instance_name(declaration.provider.as_str(), project, Some(name))
            })
        });
    Ok((instances, default_name))
}

pub fn handle_declared_project_list(config: &VmConfig, raw: bool) -> VmResult<()> {
    let (instances, default_name) = collect_declared_project_instances(config)?;
    if instances.is_empty() {
        vm_println!("No environments found");
        return Ok(());
    }
    if raw {
        render_raw_instance_table(instances, default_name.as_deref());
    } else {
        render_instance_table(instances, default_name.as_deref(), false);
    }
    Ok(())
}

/// Handle VM listing with enhanced filtering options
pub fn handle_list_enhanced(
    configured_provider: Option<&dyn InstanceProvider>,
    provider_filter: Option<&str>,
    project_config: Option<&VmConfig>,
    raw: bool,
    default_name: Option<&str>,
) -> VmResult<()> {
    let all_instances =
        collect_list_instances(configured_provider, provider_filter, project_config)?;
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
        render_instance_table(all_instances, default_name, project_config.is_none());
    }
    Ok(())
}

pub fn collect_list_instances(
    configured_provider: Option<&dyn InstanceProvider>,
    provider_filter: Option<&str>,
    project_config: Option<&VmConfig>,
) -> VmResult<Vec<InstanceInfo>> {
    let span = info_span!("vm_operation", operation = "list");
    let _enter = span.enter();
    debug!(
        "Listing VMs with enhanced filtering - provider_filter: {:?}",
        provider_filter
    );

    // Use the loaded project provider when available so provider-specific
    // settings such as Tart's storage path remain in effect.
    let mut all_instances = load_instances(configured_provider, provider_filter)?;

    if let Some(config) = project_config {
        all_instances =
            filter_project_instances(all_instances, &FleetProject::new(config.clone())?)?;
    }

    Ok(all_instances)
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

pub fn render_instance_table(
    instances: Vec<InstanceInfo>,
    default_name: Option<&str>,
    show_project: bool,
) {
    if show_project {
        vm_println!(
            "{:<20} {:<9} {:<12} {:<12} {:<10} {:<15}",
            "ENVIRONMENT",
            "DEFAULT",
            "KIND",
            "STATUS",
            "UPTIME",
            "PROJECT"
        );
        vm_println!("{}", "─".repeat(85));
    } else {
        vm_println!(
            "{:<20} {:<9} {:<12} {:<12} {:<10}",
            "ENVIRONMENT",
            "DEFAULT",
            "KIND",
            "STATUS",
            "UPTIME"
        );
        vm_println!("{}", "─".repeat(69));
    }

    let mut sorted_instances = instances;
    sorted_instances.sort_by(|a, b| a.name.cmp(&b.name));

    for instance in sorted_instances {
        let default = if default_name == Some(instance.name.as_str()) {
            "yes"
        } else {
            ""
        };
        if show_project {
            vm_println!(
                "{:<20} {:<9} {:<12} {:<12} {:<10} {:<15}",
                truncate_string(&instance.name, 20),
                default,
                format_kind(&instance),
                format_status(&instance.status),
                format_uptime(&instance.uptime),
                instance.project.as_deref().unwrap_or("--")
            );
        } else {
            vm_println!(
                "{:<20} {:<9} {:<12} {:<12} {:<10}",
                truncate_string(&instance.name, 20),
                default,
                format_kind(&instance),
                format_status(&instance.status),
                format_uptime(&instance.uptime)
            );
        }
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
    } else if lower_status == "declared" {
        "📋 Declared".to_string()
    } else if lower_status == "unavailable" {
        "⚠️  Unavailable".to_string()
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
    use vm_provider::MockProvider;

    #[test]
    fn project_listing_uses_the_configured_provider() {
        let provider = MockProvider;

        let instances = load_instances(Some(&provider), None).unwrap();

        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].name, "mock-vm");
    }
}
