mod guest;
mod updates;

use std::path::PathBuf;

use vm_config::config::VmConfig;
use vm_core::{vm_hint, vm_println, vm_success};
use vm_packages::{RegisterTool, ToolKind};
use vm_provider::Provider;

use crate::cli::ToolsSubcommand;
use crate::error::{VmError, VmResult};

use super::command_context::{
    load_or_create_runtime_subject, load_runtime_subject, RuntimeSubject,
};
use super::packages::tooling::{self, CachedToolCatalog, RefreshOutcome};
use super::vm_ops::ensure_running;

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
            let definitions = tooling::client()?.tools().await?;
            if definitions.is_empty() {
                vm_println!("No tools are registered");
            } else {
                for definition in definitions {
                    vm_println!(
                        "{}\t{}\t{}",
                        definition.name,
                        kind_name(definition.kind),
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
            show_status(&subject)
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
        let selected = updates::plan(config, &catalog.artifacts, &installed).selected(false)?;
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
    let installed = guest::installed(provider, environment)?;
    let selected = updates::plan(config, &catalog.artifacts, &installed).selected(all)?;
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
        mode,
    )?;
    match mode {
        InstallMode::Background => vm_success!("Started {count} tool update(s) in the background"),
        InstallMode::Wait => vm_success!("Activated {count} tool update(s)"),
    }
    Ok(())
}

fn show_status(subject: &RuntimeSubject) -> VmResult<()> {
    let target = guest::platform_target(subject.provider.as_ref(), &subject.target)?;
    let installed = guest::installed(subject.provider.as_ref(), &subject.target)?;
    vm_println!("Guest tools ({target})");
    if installed.is_empty() {
        vm_println!("  No managed tools are active");
    } else {
        for tool in installed.values() {
            vm_println!("  {} {} ({})", tool.name, tool.version, tool.target);
        }
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
