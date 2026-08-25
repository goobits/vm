pub(in crate::commands) mod activation;
mod catalog;
mod guest;
mod status;
mod updates;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use vm_config::{
    config::{ToolConfig, ToolsConfig, VmConfig},
    GlobalConfig,
};
use vm_core::{vm_hint, vm_println, vm_success};
use vm_packages::{RegisterTool, ToolKind};
use vm_provider::{Provider, ProviderContext};

use crate::cli::ToolsSubcommand;
use crate::error::{VmError, VmResult};

use super::command_context::{load_runtime_subject, RuntimeSubject};
use super::packages::tooling::{self, CachedToolCatalog, RefreshOutcome};
use super::vm_ops::ensure_running;
use super::{base, packages};

use guest::InstallMode;

pub(super) async fn handle(
    command: ToolsSubcommand,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        ToolsSubcommand::Register {
            name,
            repository,
            branch,
            kind,
        } => {
            if base::is_vendor_tool(&name) {
                return Err(VmError::validation(
                    format!("'{name}' is reserved for a VM-owned vendor tool"),
                    Some(format!("Use `vm tools update {name}`")),
                ));
            }
            let kind = match kind.as_str() {
                "binary" => ToolKind::Binary,
                "collection" => ToolKind::Collection,
                _ => unreachable!("clap validates tool kinds"),
            };
            let definition = tooling::client()?
                .register_tool(&RegisterTool {
                    name,
                    kind,
                    repository,
                    default_branch: branch,
                    build_sources: Vec::new(),
                    workspace_release: false,
                })
                .await?;
            vm_success!("Registered tool '{}'", definition.name);
            Ok(())
        }
        ToolsSubcommand::List => {
            let client = tooling::client()?;
            let definitions = client.tools().await?;
            vm_println!("NAME\tKIND\tREGISTERED\tPUBLISHED\tINSTALLED\tCONSUMABLE\tSOURCE");
            for definition in base::vendor_tool_info() {
                vm_println!(
                    "{}\tvendor\tn/a\tn/a\tn/a\tn/a\t{}",
                    definition.name,
                    definition.installer_url
                );
            }
            for definition in definitions
                .into_iter()
                .filter(|definition| !base::is_vendor_tool(&definition.name))
            {
                let published = !client.tool(&definition.name).await?.artifacts.is_empty();
                vm_println!(
                    "{}\t{}\tyes\t{}\tn/a\t{}\t{}",
                    definition.name,
                    kind_name(definition.kind),
                    yes_no(published),
                    "n/a",
                    definition.repository
                );
            }
            Ok(())
        }
        ToolsSubcommand::Show { name } => {
            if let Some(definition) = base::vendor_tool_info().find(|tool| tool.name == name) {
                vm_println!(
                    "{} (vendor)\n  source: {}\n  owner: VM base runtime",
                    definition.name,
                    definition.installer_url
                );
                return Ok(());
            }
            let inventory = tooling::client()?.tool(&name).await?;
            vm_println!(
                "{} ({})\n  source: {}\n  branch: {}",
                inventory.definition.name,
                kind_name(inventory.definition.kind),
                inventory.definition.repository,
                inventory.definition.default_branch
            );
            for artifact in inventory.artifacts {
                artifact.validate().map_err(VmError::from)?;
                vm_println!(
                    "  {} {} {}",
                    artifact.version,
                    artifact.target,
                    &artifact.artifact_digest[..12]
                );
            }
            Ok(())
        }
        ToolsSubcommand::Refresh { quiet } => {
            let config = vm_config::AppConfig::load(config_path, profile, None)?.vm;
            match tooling::refresh(&config).await? {
                RefreshOutcome::Refreshed if !quiet => vm_success!("Tool catalog refreshed"),
                RefreshOutcome::AlreadyRunning if !quiet => {
                    vm_println!("A tool catalog refresh is already running")
                }
                _ => {}
            }
            Ok(())
        }
        ToolsSubcommand::Status { environment } => {
            let subject = load_runtime_subject(config_path, profile, environment)?;
            status::show(&subject).await
        }
        ToolsSubcommand::Enable { tools } => {
            set_global_selection(&tools, true)?;
            activation::ensure_worker()?;
            vm_success!("Enabled globally: {}", tools.join(", "));
            updates::run(
                config_path,
                profile,
                Vec::new(),
                Vec::new(),
                false,
                InstallMode::Wait,
            )
            .await
        }
        ToolsSubcommand::ActivationWorker { once } => activation::run_worker(once).await,
        ToolsSubcommand::Disable { tools } => {
            set_global_selection(&tools, false)?;
            if GlobalConfig::load()?.tools.is_empty() {
                activation::remove_worker()?;
            }
            vm_success!("Disabled globally: {}", tools.join(", "));
            vm_hint!(
                "Existing managed installations are retained but no longer receive global updates"
            );
            Ok(())
        }
        ToolsSubcommand::Update {
            tools,
            to,
            include_stopped,
            background,
        } => {
            let mode = if background {
                InstallMode::Background
            } else {
                InstallMode::Wait
            };
            updates::run(config_path, profile, tools, to, include_stopped, mode).await
        }
    }
}

fn set_global_selection(names: &[String], enabled: bool) -> VmResult<()> {
    let mut global = GlobalConfig::load().map_err(VmError::from)?;
    apply_global_selection(&mut global, names, enabled)?;
    global.save().map_err(VmError::from)
}

fn apply_global_selection(
    global: &mut GlobalConfig,
    names: &[String],
    enabled: bool,
) -> VmResult<()> {
    let vendor_tools = names
        .iter()
        .filter(|name| base::is_vendor_tool(name))
        .cloned()
        .collect::<Vec<_>>();
    if !vendor_tools.is_empty() {
        return Err(VmError::validation(
            format!(
                "VM-owned vendor tools do not require global selection: {}",
                vendor_tools.join(", ")
            ),
            Some(format!(
                "Use `vm tools update {}` to update them",
                vendor_tools.join(" ")
            )),
        ));
    }
    let proposed = names
        .iter()
        .map(|name| (name.clone(), ToolConfig::default()))
        .collect();
    ToolsConfig {
        entries: proposed,
        ..Default::default()
    }
    .validate()
    .map_err(VmError::from)?;

    for name in names {
        if enabled {
            global.tools.entry(name.clone()).or_default();
        } else {
            global.tools.shift_remove(name);
        }
    }
    Ok(())
}

async fn reconcile_subject(subject: &RuntimeSubject) -> VmResult<()> {
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

fn reconcile_environment(subject: &RuntimeSubject) -> VmResult<()> {
    let context = ProviderContext::default().with_config(subject.global_config.clone());
    subject
        .provider
        .reconcile_runtime(Some(&subject.target), &context)
        .map_err(VmError::from)?;
    reconcile_guest_settings(subject.provider.as_ref(), &subject.target, &subject.config)?;
    base::reconcile_vendor_tools(subject.provider.as_ref(), &subject.target, &subject.config)
}

fn reconcile_guest_settings(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    packages::reconcile_client_settings(provider, environment, config)?;
    super::managed_guest::reconcile_remote_commands(provider, environment)
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn kind_name(kind: ToolKind) -> &'static str {
    match kind {
        ToolKind::Binary => "binary",
        ToolKind::Collection => "collection",
    }
}

/// Fast interactive-shell hook: cached local state only, with refresh detached.
pub(in crate::commands) fn before_shell(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
) {
    if config.tools.entries.is_empty() {
        return;
    }
    let has_catalog = tooling::has_fresh_catalog();
    if let Err(error) = tooling::refresh_in_background(config.clone()) {
        tracing::debug!(%error, "Could not schedule tool catalog refresh");
    }
    if !has_catalog {
        return;
    }

    let result = (|| -> VmResult<()> {
        let state = guest::shell_state(provider, environment)?;
        let Some(catalog) = tooling::cached(config, &state.target)? else {
            return Ok(());
        };
        report_missing(&catalog);
        let selected = updates::plan(
            config,
            &catalog.artifacts,
            &state.installed,
            &state.consumable,
        )
        .automatic();
        if selected.is_empty() {
            return Ok(());
        }
        guest::install(
            provider,
            environment,
            &selected,
            &tooling::gateway(provider.name())?,
            &tooling::read_token()?,
            InstallMode::BackgroundIfIdle,
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        tracing::debug!(%error, "Skipped cached guest tool activation");
    }
}

fn apply_updates(
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

fn project_workspace(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.workspace_path.as_deref())
        .unwrap_or("/workspace")
}

fn report_project_overrides(overrides: &BTreeMap<String, BTreeSet<String>>) {
    for (name, destinations) in overrides {
        vm_hint!(
            "Project-local collection '{name}' is also checked out at {} and can override the managed guest copy. VM leaves project Git unchanged; remove that checkout or update it separately.",
            destinations.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_global_selection, project_workspace};
    use vm_config::config::VmConfig;
    use vm_config::GlobalConfig;

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

    #[test]
    fn global_selection_is_idempotent_and_reversible() {
        let mut global = GlobalConfig::default();
        let names = vec!["codeatlas".to_string(), "typemill".to_string()];
        apply_global_selection(&mut global, &names, true).unwrap();
        apply_global_selection(&mut global, &names, true).unwrap();
        assert_eq!(global.tools.len(), 2);

        apply_global_selection(&mut global, &["typemill".into()], false).unwrap();
        assert!(global.tools.contains_key("codeatlas"));
        assert!(!global.tools.contains_key("typemill"));
    }

    #[test]
    fn vendor_tools_are_updated_without_global_selection() {
        let mut global = GlobalConfig::default();
        let error = apply_global_selection(&mut global, &["codex".into()], true).unwrap_err();

        assert!(error
            .to_string()
            .contains("do not require global selection"));
        assert!(global.tools.is_empty());
    }
}
