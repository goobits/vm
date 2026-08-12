mod guest;
mod reconcile;
mod updates;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use vm_config::config::VmConfig;
use vm_core::{vm_hint, vm_println, vm_success};
use vm_packages::{RegisterTool, ToolKind};
use vm_provider::Provider;

use crate::cli::ToolsSubcommand;
use crate::error::{VmError, VmResult};

use super::command_context::{
    load_or_create_runtime_subject, load_runtime_subject, RuntimeSubject,
};
use super::packages;
use super::packages::tooling::{self, CachedToolCatalog, RefreshOutcome};
use super::vm_ops::ensure_running;

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
        ToolsSubcommand::Publish { name } => {
            packages::publish_tool(&name)?;
            if let Ok(config) = vm_config::AppConfig::load(config_path, profile, None) {
                let _ = tooling::refresh(&config.vm).await;
            }
            vm_success!("Published tool '{name}'");
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
            environment,
            all,
            background,
        } => {
            let subject = load_or_create_runtime_subject(config_path, profile, environment).await?;
            ensure_running(
                subject.provider.as_ref(),
                Some(subject.target.as_str()),
                &subject.config,
                &subject.global_config,
                true,
            )
            .await?;
            reconcile::environment(&subject)?;
            ensure_builtin_releases(&subject.config).await?;
            tooling::refresh(&subject.config).await?;
            apply_updates(
                subject.provider.as_ref(),
                &subject.target,
                &subject.config,
                all,
                if background {
                    InstallMode::Background
                } else {
                    InstallMode::Wait
                },
            )
        }
    }
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
                        "Run `gh auth login --hostname github.com`, then `vm packages auth --github`, then `vm tools publish {}`",
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
                    "Run `vm tools publish {}`, then retry `vm tools update`",
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
    let refresh_config = config.clone();
    tokio::spawn(async move {
        let _ = tooling::refresh(&refresh_config).await;
    });
    if !has_catalog {
        return;
    }

    let result = (|| -> VmResult<()> {
        let target = guest::platform_target(provider, environment)?;
        let Some(catalog) = tooling::cached(config, &target)? else {
            return Ok(());
        };
        report_missing(&catalog);
        let installed = guest::installed(provider, environment)?;
        let consumable = guest::consumable(provider, environment)?;
        let selected =
            updates::plan(config, &catalog.artifacts, &installed, &consumable).selected(false)?;
        if selected.is_empty() {
            return Ok(());
        }
        let names = selected
            .iter()
            .map(|artifact| artifact.tool.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        guest::install(
            provider,
            environment,
            &selected,
            &tooling::gateway(provider.name())?,
            &tooling::read_token()?,
            InstallMode::Background,
        )?;
        vm_println!("Tool updates started in the background: {names}");
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
    all: bool,
    mode: InstallMode,
) -> VmResult<()> {
    let target = guest::platform_target(provider, environment)?;
    let catalog = tooling::cached(config, &target)?.ok_or_else(|| {
        VmError::validation(
            "The tool catalog cache is unavailable",
            Some("Run `vm tools refresh` and retry"),
        )
    })?;
    report_missing(&catalog);
    let project_overrides = guest::project_collection_overrides(
        provider,
        environment,
        project_workspace(config),
        &catalog.artifacts,
    )?;
    report_project_overrides(&project_overrides);
    let installed = guest::installed(provider, environment)?;
    let consumable = guest::consumable(provider, environment)?;
    let selected =
        updates::plan(config, &catalog.artifacts, &installed, &consumable).selected(all)?;
    if selected.is_empty() {
        vm_success!("Configured tools are current");
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
                Some(format!(
                    "Run `vm tools update {environment} --all` to retry"
                )),
            ));
        }
    }
    match mode {
        InstallMode::Background => vm_success!("Started {count} tool update(s) in the background"),
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
    let codex = reconcile::codex_state(subject.provider.as_ref(), &subject.target)?;

    vm_println!("Guest tools ({target})");
    vm_println!("NAME\tOWNER\tREGISTERED\tPUBLISHED\tINSTALLED\tCONSUMABLE\tPROJECT_COPY\tVERSION");
    if reconcile::codex_expected(&subject.config) || codex != reconcile::CodexState::Absent {
        vm_println!(
            "codex\tbase\tn/a\tn/a\t{}\t{}\tn/a\t-",
            yes_no(codex != reconcile::CodexState::Absent),
            yes_no(codex == reconcile::CodexState::Consumable)
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
    use super::{project_workspace, tool_status_names, ControllerToolState};
    use crate::commands::tools::guest::InstalledTool;
    use std::collections::BTreeMap;
    use vm_config::config::{ToolConfig, VmConfig};

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
}
