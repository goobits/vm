use std::path::PathBuf;

use vm_config::{
    config::{ToolConfig, ToolsConfig},
    GlobalConfig,
};
use vm_core::{vm_hint, vm_println, vm_success};
use vm_packages::{RegisterTool, ToolKind};

use crate::cli::ToolsSubcommand;
use crate::error::{VmError, VmResult};

use crate::commands::base;
use crate::commands::command_context::load_runtime_subject;
use crate::commands::packages::tooling::{self, RefreshOutcome};

use super::{activation, guest::InstallMode, status, updates};

pub(in crate::commands) async fn handle(
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
                tools,
                Vec::new(),
                false,
                InstallMode::Wait,
            )
            .await
        }
        ToolsSubcommand::ActivationWorker { once } => activation::run_worker(once).await,
        ToolsSubcommand::ReconcileWorker { environment } => {
            super::background::run(&environment).await
        }
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

pub(super) fn yes_no(value: bool) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::apply_global_selection;
    use vm_config::GlobalConfig;

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
