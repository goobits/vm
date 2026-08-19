mod guest;
mod updates;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use vm_config::{config::VmConfig, AppConfig};
use vm_core::{vm_hint, vm_println, vm_success};
use vm_packages::{RegisterTool, ToolKind};
use vm_provider::{InstanceInfo, Provider, ProviderContext};

use crate::cli::{FleetArgs, ToolsSubcommand};
use crate::error::{VmError, VmResult};

use super::command_context::{
    load_runtime_subject, load_runtime_subject_for_instance, RuntimeSubject,
};
use super::packages::tooling::{self, CachedToolCatalog, RefreshOutcome};
use super::vm_ops::{self, ensure_running, FleetProgress, InstanceStateFilter};
use super::{base, packages};

use guest::InstallMode;

struct BuiltinTool {
    name: &'static str,
    kind: ToolKind,
    repository: &'static str,
    branch: &'static str,
    requires_git_auth: bool,
}

const BUILTIN_TOOLS: &[BuiltinTool] = &[BuiltinTool {
    name: "agent-skills",
    kind: ToolKind::Collection,
    repository: "https://github.com/goobits/agent-skills.git",
    branch: "main",
    requires_git_auth: true,
}];

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
                    workspace_release: false,
                })
                .await?;
            vm_success!("Registered tool '{}'", definition.name);
            Ok(())
        }
        ToolsSubcommand::List => {
            let client = tooling::client()?;
            let definitions = client.tools().await?;
            if definitions.is_empty() {
                vm_println!("No tools are registered");
            } else {
                vm_println!("NAME\tKIND\tREGISTERED\tPUBLISHED\tINSTALLED\tCONSUMABLE\tSOURCE");
                for definition in definitions {
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
            }
            Ok(())
        }
        ToolsSubcommand::Show { name } => {
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
            show_status(&subject).await
        }
        ToolsSubcommand::Update {
            tools,
            to,
            include_stopped,
            fleet,
            background,
        } => {
            let mode = if background {
                InstallMode::Background
            } else {
                InstallMode::Wait
            };
            if let Some(environment) =
                legacy_environment_target(&tools, &to, include_stopped, &fleet)?
            {
                let subject = load_runtime_subject(config_path, profile, Some(environment))?;
                reconcile_subject(&subject).await?;
                prepare_tool_catalog(&subject.config).await?;
                apply_updates(
                    subject.provider.as_ref(),
                    &subject.target,
                    &subject.config,
                    mode,
                    false,
                )
            } else if fleet.fleet {
                if !tools.is_empty() || !to.is_empty() || include_stopped {
                    return Err(VmError::validation(
                        "Legacy --fleet targeting cannot be combined with tool selectors",
                        Some(
                            "Remove --fleet; use `vm tools update [TOOL...] [--to ENVIRONMENT...]`",
                        ),
                    ));
                }
                update_legacy_fleet(config_path, profile, &fleet, mode).await
            } else {
                update_running_docker(config_path, profile, &tools, &to, include_stopped, mode)
                    .await
            }
        }
    }
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

async fn prepare_tool_catalog(config: &VmConfig) -> VmResult<()> {
    if config.tools.entries.is_empty() {
        return Ok(());
    }
    ensure_builtin_releases(config).await?;
    tooling::refresh(config).await?;
    Ok(())
}

fn legacy_environment_target(
    tools: &[String],
    to: &[String],
    include_stopped: bool,
    fleet: &FleetArgs,
) -> VmResult<Option<String>> {
    if tools.len() != 1
        || !to.is_empty()
        || include_stopped
        || fleet.fleet
        || fleet.provider.is_some()
        || fleet.pattern.is_some()
    {
        return Ok(None);
    }
    let probe = FleetArgs {
        fleet: true,
        provider: None,
        pattern: Some(tools[0].clone()),
    };
    let matches = vm_ops::resolve_fleet_targets(&probe, InstanceStateFilter::Any)?;
    Ok(matches
        .into_iter()
        .find(|instance| instance.name == tools[0])
        .map(|instance| instance.name))
}

async fn update_running_docker(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    tools: &[String],
    environments: &[String],
    include_stopped: bool,
    mode: InstallMode,
) -> VmResult<()> {
    let targets = FleetArgs {
        fleet: true,
        provider: Some("docker".into()),
        pattern: None,
    };
    let state = if include_stopped {
        InstanceStateFilter::Any
    } else {
        InstanceStateFilter::Running
    };
    let instances = select_named_targets(
        vm_ops::resolve_fleet_targets(&targets, state)?,
        environments,
    )?;
    if instances.is_empty() {
        vm_println!(
            "No {}managed Docker environments found",
            if include_stopped { "" } else { "running " }
        );
        return Ok(());
    }

    let mut progress = FleetProgress::default();
    for instance in instances {
        let result =
            update_owned_target(config_path.clone(), profile.clone(), &instance, tools, mode).await;
        match result {
            Ok(()) => progress.success(&instance.name),
            Err(error) => progress.failure(&instance.name, &error),
        }
    }
    progress.finish()
}

fn select_named_targets(
    instances: Vec<InstanceInfo>,
    requested: &[String],
) -> VmResult<Vec<InstanceInfo>> {
    if requested.is_empty() {
        return Ok(instances);
    }
    let mut available = instances
        .into_iter()
        .map(|instance| (instance.name.clone(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    let mut missing = Vec::new();
    for name in requested {
        match available.remove(name) {
            Some(instance) => selected.push(instance),
            None if !missing.contains(name) => missing.push(name.clone()),
            None => {}
        }
    }
    if !missing.is_empty() {
        return Err(VmError::validation(
            format!(
                "Managed Docker environment{} not found or not running: {}",
                if missing.len() == 1 { "" } else { "s" },
                missing.join(", ")
            ),
            Some("Use `vm list --all` or add --include-stopped"),
        ));
    }
    Ok(selected)
}

async fn update_owned_target(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    instance: &InstanceInfo,
    tools: &[String],
    mode: InstallMode,
) -> VmResult<()> {
    let mut subject = load_runtime_subject_for_instance(config_path, profile, instance)?;
    select_tools(&mut subject.config, tools);
    reconcile_subject(&subject).await?;
    prepare_tool_catalog(&subject.config).await?;
    apply_updates(
        subject.provider.as_ref(),
        &subject.target,
        &subject.config,
        mode,
        !tools.is_empty(),
    )
}

fn select_tools(config: &mut VmConfig, requested: &[String]) {
    if requested.is_empty() {
        return;
    }
    let configured = config.tools.entries.clone();
    config.tools.entries.clear();
    for name in requested {
        if config.tools.entries.contains_key(name) {
            continue;
        }
        config.tools.entries.insert(
            name.clone(),
            configured.get(name).cloned().unwrap_or_default(),
        );
    }
}

async fn update_legacy_fleet(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    fleet: &FleetArgs,
    mode: InstallMode,
) -> VmResult<()> {
    let app_config = AppConfig::load(config_path, profile, None)?;
    let config = app_config.vm;
    let instances = vm_ops::resolve_fleet_targets(fleet, InstanceStateFilter::Any)?;
    if instances.is_empty() {
        vm_println!("No managed environments found");
        return Ok(());
    }

    update_targets(&config, instances, mode).await
}

async fn update_targets(
    config: &VmConfig,
    instances: Vec<InstanceInfo>,
    mode: InstallMode,
) -> VmResult<()> {
    prepare_tool_catalog(config).await?;
    let mut progress = FleetProgress::default();
    for instance in instances {
        match update_fleet_target(config, &instance, mode).await {
            Ok(()) => progress.success(&instance.name),
            Err(error) => progress.failure(&instance.name, &error),
        }
    }
    progress.finish()
}

async fn update_fleet_target(
    config: &VmConfig,
    instance: &InstanceInfo,
    mode: InstallMode,
) -> VmResult<()> {
    let mut config = config_for_fleet_target(config, instance);
    packages::apply_client_environment(&mut config)?;
    let provider = vm_ops::configured_provider(&config, &instance.provider)?;
    provider
        .start(Some(&instance.name), &ProviderContext::default())
        .map_err(VmError::from)?;
    vm_ops::wait_until_commands_ready(provider.as_ref(), Some(&instance.name), &instance.name)
        .await?;
    reconcile_guest_settings(provider.as_ref(), &instance.name, &config)?;
    base::reconcile_codex(provider.as_ref(), &instance.name, &config)?;
    apply_updates(provider.as_ref(), &instance.name, &config, mode, false)
}

fn config_for_fleet_target(config: &VmConfig, instance: &InstanceInfo) -> VmConfig {
    let mut config = config.clone();
    config.provider = Some(instance.provider.clone());
    if let Some(project_name) = &instance.project {
        config.project.get_or_insert_with(Default::default).name = Some(project_name.clone());
    }
    config
}

fn reconcile_environment(subject: &RuntimeSubject) -> VmResult<()> {
    let context = ProviderContext::default().with_config(subject.global_config.clone());
    subject
        .provider
        .reconcile_runtime(Some(&subject.target), &context)
        .map_err(VmError::from)?;
    reconcile_guest_settings(subject.provider.as_ref(), &subject.target, &subject.config)?;
    base::reconcile_codex(subject.provider.as_ref(), &subject.target, &subject.config)
}

fn reconcile_guest_settings(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    packages::reconcile_client_settings(provider, environment, config)?;
    super::managed_guest::reconcile_remote_commands(provider, environment)
}

async fn ensure_builtin_releases(config: &VmConfig) -> VmResult<()> {
    let selected = BUILTIN_TOOLS
        .iter()
        .filter(|tool| config.tools.entries.contains_key(tool.name))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(());
    }
    let client = tooling::client()?;
    let definitions = client.tools().await?;
    for tool in selected {
        if !definitions
            .iter()
            .any(|definition| definition.name == tool.name)
        {
            client
                .register_tool(&RegisterTool {
                    name: tool.name.into(),
                    kind: tool.kind,
                    repository: tool.repository.into(),
                    default_branch: tool.branch.into(),
                    workspace_release: false,
                })
                .await?;
            vm_success!("Registered built-in tool '{}'", tool.name);
        }

        let inventory = client.tool(tool.name).await?;
        let configured_version = config.tools.entries[tool.name]
            .version
            .as_deref()
            .filter(|version| *version != "latest");
        let has_release = configured_version.map_or_else(
            || !inventory.artifacts.is_empty(),
            |version| {
                inventory
                    .artifacts
                    .iter()
                    .any(|artifact| artifact.version == version)
            },
        );
        if !has_release {
            if tool.requires_git_auth
                && inventory.definition.repository == tool.repository
                && !packages::git_auth_configured()?
            {
                return Err(VmError::validation(
                    format!("Built-in tool '{}' requires private Git access", tool.name),
                    Some(format!(
                        "Configure controller Git access, then create and release a managed '{}' checkout from a writable environment",
                        tool.name
                    )),
                ));
            }
            return Err(VmError::validation(
                format!(
                    "Built-in tool '{}' is registered but not published",
                    tool.name
                ),
                Some(format!(
                    "Create and release a managed '{}' checkout from a writable environment, then retry `vm tools update`",
                    tool.name
                )),
            ));
        }
    }
    Ok(())
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
    report_missing(&catalog);
    if explicitly_selected {
        let missing = catalog
            .missing
            .iter()
            .filter(|name| config.tools.entries.contains_key(*name))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(VmError::validation(
                format!(
                    "No published compatible release exists for: {}",
                    missing.join(", ")
                ),
                Some("Run `vm tools list` to inspect registered and published tools"),
            ));
        }
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
    let selected = updates::plan(config, &catalog.artifacts, &installed, &consumable).eligible();
    if selected.is_empty() {
        vm_success!(
            "{} tools are current",
            if explicitly_selected {
                "Selected"
            } else {
                "Configured"
            }
        );
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

#[derive(Debug, Clone, Copy, Default)]
struct ControllerToolState {
    registered: bool,
    published: bool,
}

async fn controller_tool_states() -> VmResult<BTreeMap<String, ControllerToolState>> {
    let client = tooling::client()?;
    let definitions = client.tools().await?;
    let mut states = BTreeMap::new();
    for definition in definitions {
        let published = !client.tool(&definition.name).await?.artifacts.is_empty();
        states.insert(
            definition.name,
            ControllerToolState {
                registered: true,
                published,
            },
        );
    }
    Ok(states)
}

async fn show_status(subject: &RuntimeSubject) -> VmResult<()> {
    let target = guest::platform_target(subject.provider.as_ref(), &subject.target)?;
    let catalog = tooling::cached(&subject.config, &target)?;
    let project_overrides = match &catalog {
        Some(catalog) => guest::project_collection_overrides(
            subject.provider.as_ref(),
            &subject.target,
            project_workspace(&subject.config),
            &catalog.artifacts,
        )?,
        None => BTreeMap::new(),
    };
    let installed = guest::installed(subject.provider.as_ref(), &subject.target)?;
    let consumable = guest::consumable(subject.provider.as_ref(), &subject.target)?;
    let controller = match controller_tool_states().await {
        Ok(states) => Some(states),
        Err(error) => {
            vm_hint!("Controller package state is unavailable: {error}");
            None
        }
    };
    let codex = base::codex_state(subject.provider.as_ref(), &subject.target)?;

    vm_println!("Guest tools ({target})");
    vm_println!("NAME\tOWNER\tREGISTERED\tPUBLISHED\tINSTALLED\tCONSUMABLE\tPROJECT_COPY\tVERSION");
    if base::codex_expected(&subject.config) || codex != base::CodexState::Absent {
        vm_println!(
            "codex\tbase\tn/a\tn/a\t{}\t{}\tn/a\t-",
            yes_no(codex != base::CodexState::Absent),
            yes_no(codex == base::CodexState::Consumable)
        );
    }
    for name in tool_status_names(
        &subject.config,
        controller.as_ref(),
        &installed,
        &consumable,
    ) {
        let controller_state = controller.as_ref().and_then(|states| states.get(&name));
        let installed_tool = installed.get(&name);
        vm_println!(
            "{}\tmanaged\t{}\t{}\t{}\t{}\t{}\t{}",
            name,
            controller_state.map_or("unknown", |state| yes_no(state.registered)),
            controller_state.map_or("unknown", |state| yes_no(state.published)),
            yes_no(installed_tool.is_some()),
            yes_no(consumable.get(&name).copied().unwrap_or(false)),
            if catalog.is_some() {
                yes_no(project_overrides.contains_key(&name))
            } else {
                "unknown"
            },
            installed_tool.map_or("-", |tool| tool.version.as_str())
        );
    }
    report_project_overrides(&project_overrides);
    Ok(())
}

fn tool_status_names(
    config: &VmConfig,
    controller: Option<&BTreeMap<String, ControllerToolState>>,
    installed: &BTreeMap<String, guest::InstalledTool>,
    consumable: &BTreeMap<String, bool>,
) -> BTreeSet<String> {
    let mut names = config
        .tools
        .entries
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    if let Some(controller) = controller {
        names.extend(controller.keys().cloned());
    }
    names.extend(installed.keys().cloned());
    names.extend(consumable.keys().cloned());
    names
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
    use super::{
        config_for_fleet_target, project_workspace, select_named_targets, select_tools,
        tool_status_names, ControllerToolState,
    };
    use crate::commands::tools::guest::InstalledTool;
    use std::collections::BTreeMap;
    use vm_config::config::{ToolConfig, VmConfig};
    use vm_provider::InstanceInfo;

    #[test]
    fn status_includes_controller_and_stale_guest_state() {
        let mut config = VmConfig::default();
        config
            .tools
            .entries
            .insert("configured".into(), ToolConfig::default());
        let controller = BTreeMap::from([(
            "registered".into(),
            ControllerToolState {
                registered: true,
                published: false,
            },
        )]);
        let installed = BTreeMap::from([(
            "stale-installed".into(),
            InstalledTool {
                name: "stale-installed".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);
        let consumable = BTreeMap::from([("orphan-state".into(), false)]);

        let names = tool_status_names(&config, Some(&controller), &installed, &consumable);

        assert_eq!(
            names.into_iter().collect::<Vec<_>>(),
            [
                "configured",
                "orphan-state",
                "registered",
                "stale-installed"
            ]
        );
    }

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
    fn explicit_tool_selection_keeps_project_pins_and_adds_unconfigured_tools() {
        let mut config = VmConfig::default();
        config.tools.entries.insert(
            "agent-skills".into(),
            ToolConfig {
                version: Some("0.8.0".into()),
                updates: None,
            },
        );
        config
            .tools
            .entries
            .insert("unrelated".into(), ToolConfig::default());

        select_tools(
            &mut config,
            &["agent-skills".into(), "helper".into(), "helper".into()],
        );

        assert_eq!(
            config.tools.entries.keys().cloned().collect::<Vec<_>>(),
            ["agent-skills", "helper"]
        );
        assert_eq!(
            config.tools.entries["agent-skills"].version.as_deref(),
            Some("0.8.0")
        );
        assert_eq!(config.tools.entries["helper"], ToolConfig::default());
    }

    #[test]
    fn named_targets_preserve_request_order_and_reject_missing_environments() {
        let instance = |name: &str| InstanceInfo {
            name: name.into(),
            id: format!("{name}-id"),
            status: "running".into(),
            provider: "docker".into(),
            project: Some(name.into()),
            uptime: None,
            created_at: None,
        };
        let selected = select_named_targets(
            vec![instance("api-dev"), instance("web-dev")],
            &["web-dev".into(), "api-dev".into()],
        )
        .unwrap();
        assert_eq!(
            selected
                .into_iter()
                .map(|target| target.name)
                .collect::<Vec<_>>(),
            ["web-dev", "api-dev"]
        );

        let error =
            select_named_targets(vec![instance("api-dev")], &["missing-dev".into()]).unwrap_err();
        assert!(error.to_string().contains("missing-dev"));
    }

    #[test]
    fn fleet_target_uses_its_provider_and_project_identity() {
        let mut config = VmConfig::default();
        config
            .tools
            .entries
            .insert("agent-skills".into(), ToolConfig::default());
        let target = InstanceInfo {
            name: "store-dev".into(),
            id: "container-1".into(),
            status: "running".into(),
            provider: "docker".into(),
            project: Some("store".into()),
            uptime: None,
            created_at: None,
        };

        let resolved = config_for_fleet_target(&config, &target);

        assert_eq!(resolved.provider.as_deref(), Some("docker"));
        assert_eq!(
            resolved.project.and_then(|project| project.name),
            Some("store".into())
        );
        assert!(resolved.tools.entries.contains_key("agent-skills"));
    }
}
