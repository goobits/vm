use std::collections::{BTreeMap, BTreeSet};

use vm_config::config::VmConfig;
use vm_core::{vm_hint, vm_success};
use vm_provider::{Provider, ProviderContext};

use crate::commands::command_context::RuntimeSubject;
use crate::commands::vm_ops::ensure_running;
use crate::commands::{base, packages};
use crate::error::{VmError, VmResult};

use super::guest::{self, InstallMode};
use super::updates;
use crate::commands::packages::tooling::{self, CachedToolCatalog};

pub(super) async fn reconcile_subject(subject: &RuntimeSubject) -> VmResult<()> {
    ensure_running(
        subject.provider.as_ref(),
        Some(subject.target.as_str()),
        &subject.config,
        &subject.global_config,
        true,
    )
    .await?;
    reconcile_environment(subject)
}

pub(super) fn reconcile_environment(subject: &RuntimeSubject) -> VmResult<()> {
    reconcile_managed_guest(
        subject.provider.as_ref(),
        Some(&subject.target),
        &subject.target,
        &subject.config,
        &subject.global_config,
    )?;
    base::reconcile_vendor_tools(subject.provider.as_ref(), &subject.target, &subject.config)
}

pub(in crate::commands) fn reconcile_managed_guest(
    provider: &dyn Provider,
    target: Option<&str>,
    environment: &str,
    config: &VmConfig,
    global_config: &vm_config::GlobalConfig,
) -> VmResult<()> {
    let context = ProviderContext::default().with_config(global_config.clone());
    provider
        .reconcile_runtime(target, &context)
        .map_err(VmError::from)?;
    if config.package_edge.is_some() {
        packages::reconcile_client_settings(provider, environment, config)?;
    }
    crate::commands::managed_guest::reconcile_remote_commands(provider, environment)
}

pub(super) fn apply_updates(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
    mode: InstallMode,
    explicitly_selected: bool,
) -> VmResult<()> {
    if config.tools.entries.is_empty() {
        vm_success!("No managed tools are configured");
        return Ok(());
    }
    let target = guest::platform_target(provider, environment)?;
    let catalog = tooling::cached(config, &target)?.ok_or_else(|| {
        VmError::validation(
            "The tool catalog cache is unavailable",
            Some("Run `vm tools refresh` and retry"),
        )
    })?;
    if explicitly_selected {
        if !catalog.missing.is_empty() {
            return Err(VmError::validation(
                format!(
                    "No cached compatible release is available for: {}",
                    catalog.missing.join(", ")
                ),
                Some("Run `vm tools list` to inspect registered and published tools"),
            ));
        }
    } else {
        report_missing(&catalog);
    }
    let project_overrides = guest::project_collection_overrides(
        provider,
        environment,
        project_workspace(config),
        &catalog.artifacts,
    )?;
    report_project_overrides(&project_overrides);
    let installed = guest::installed(provider, environment)?;
    let consumable = guest::consumable(provider, environment)?;
    let plan = updates::plan(config, &catalog.artifacts, &installed, &consumable);
    let suppressed = plan
        .suppressed
        .iter()
        .map(|artifact| artifact.tool.clone())
        .collect::<Vec<_>>();
    if !suppressed.is_empty() {
        vm_hint!(
            "Update policy is off; skipped newer releases for: {}",
            suppressed.join(", ")
        );
    }
    let selected = plan.eligible();
    if selected.is_empty() {
        if suppressed.is_empty() {
            vm_success!(
                "{} tools are current",
                if explicitly_selected {
                    "Selected"
                } else {
                    "Configured"
                }
            );
        } else {
            vm_success!("No tool updates applied");
        }
        return Ok(());
    }
    let count = selected.len();
    guest::install(
        provider,
        environment,
        &selected,
        &tooling::gateway(provider.name())?,
        &tooling::read_token()?,
        mode,
    )?;
    if mode == InstallMode::Wait {
        let consumable = guest::consumable(provider, environment)?;
        let broken = selected
            .iter()
            .filter(|artifact| !consumable.get(&artifact.tool).copied().unwrap_or(false))
            .map(|artifact| artifact.tool.as_str())
            .collect::<Vec<_>>();
        if !broken.is_empty() {
            return Err(VmError::validation(
                format!("Tool activation is not consumable: {}", broken.join(", ")),
                Some(format!("Run `vm tools update --to {environment}` to retry")),
            ));
        }
    }
    match mode {
        InstallMode::Background | InstallMode::BackgroundIfIdle => {
            vm_success!("Tool reconciliation is running in the background ({count} selected)")
        }
        InstallMode::Wait => vm_success!("Activated {count} tool update(s)"),
    }
    Ok(())
}

fn report_missing(catalog: &CachedToolCatalog) {
    if !catalog.missing.is_empty() {
        vm_hint!(
            "No cached release is available for: {}",
            catalog.missing.join(", ")
        );
    }
}

pub(super) fn project_workspace(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace")
}

pub(super) fn report_project_overrides(overrides: &BTreeMap<String, BTreeSet<String>>) {
    for (name, destinations) in overrides {
        vm_hint!(
            "Project-local collection '{name}' is also checked out at {} and can override the managed guest copy. VM leaves project Git unchanged; remove that checkout or update it separately.",
            destinations.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::project_workspace;
    use vm_config::config::VmConfig;

    #[test]
    fn project_workspace_defaults_to_the_guest_mount() {
        let mut config = VmConfig::default();
        assert_eq!(project_workspace(&config), "/workspace");
        config.project = Some(vm_config::config::ProjectConfig {
            workspace_path: Some("/source".into()),
            ..Default::default()
        });
        assert_eq!(project_workspace(&config), "/source");
    }
}
