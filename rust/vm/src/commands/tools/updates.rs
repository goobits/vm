use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use semver::Version;
use vm_config::config::{ToolUpdatePolicy, VmConfig};
use vm_core::vm_println;
use vm_packages::ToolArtifactRecord;
use vm_provider::InstanceInfo;

use super::guest::{InstallMode, InstalledTool};
use super::reconcile::{apply_updates, reconcile_environment};
use super::{background, catalog};
use crate::cli::FleetArgs;
use crate::commands::base;
use crate::commands::command_context::{
    load_runtime_subject, load_runtime_subject_for_instance, RuntimeSubject,
};
use crate::commands::vm_ops::{self, FleetProgress, InstanceStateFilter};
use crate::error::{VmError, VmResult};

pub(super) struct UpdateRequest {
    pub config_path: Option<PathBuf>,
    pub profile: Option<String>,
    pub tools: Vec<String>,
    pub environments: Vec<String>,
    pub all_envs: bool,
    pub global: bool,
    pub include_stopped: bool,
    pub mode: InstallMode,
}

pub(super) async fn run(request: UpdateRequest) -> VmResult<()> {
    let UpdateRequest {
        config_path,
        profile,
        tools,
        environments,
        all_envs,
        global,
        include_stopped,
        mode,
    } = request;
    let update_all = tools.is_empty();
    let (vendor_tools, managed_tools) = partition_update_request(tools);
    let update_managed = update_all || !managed_tools.is_empty();
    let (instances, requested_tools) = resolve_request(
        config_path.clone(),
        profile.clone(),
        &managed_tools,
        &environments,
        all_envs,
        global,
        include_stopped,
    )?;
    if instances.is_empty() {
        vm_println!("No running managed environments; tool selection will apply when one starts");
        return Ok(());
    }

    let requested = requested_tools.into_iter().collect::<BTreeSet<_>>();
    let mut configured = BTreeSet::new();
    let mut subjects = Vec::new();
    let mut stopped = Vec::new();
    let mut progress = FleetProgress::default();
    let mut load_failed = false;
    for instance in instances {
        let target_config = if global { None } else { config_path.clone() };
        let target_profile = if global { None } else { profile.clone() };
        match load_runtime_subject_for_instance(target_config, target_profile, &instance) {
            Ok(mut subject) => {
                remove_stale_vendor_selections(&mut subject.config);
                if update_managed {
                    configured.extend(select_configured_tools(&mut subject.config, &requested));
                }
                if !vm_ops::is_running_status(&instance.status) {
                    stopped.push(subject);
                    continue;
                }
                subjects.push(subject);
            }
            Err(error) => {
                load_failed = true;
                progress.failure(&instance.name, &error);
            }
        }
    }

    if update_managed {
        validate_configured_selection(&requested, &configured, load_failed)?;
    }

    for subject in stopped {
        let name = subject.target.clone();
        let pending = background::PendingUpdate {
            managed: if update_managed {
                subject.config.tools.entries.keys().cloned().collect()
            } else {
                BTreeSet::new()
            },
            vendors: vendor_tools.iter().cloned().collect(),
            all_managed: update_all,
            all_vendors: update_all,
        };
        if !pending.all_managed
            && !pending.all_vendors
            && pending.managed.is_empty()
            && pending.vendors.is_empty()
        {
            vm_println!("No selected tool update for stopped environment {name}");
            progress.success(&name);
            continue;
        }
        match background::defer(subject.provider.name(), &name, pending) {
            Ok(()) => {
                vm_println!("Deferred tool update for stopped environment {name}; it will apply after start");
                progress.success(&name);
            }
            Err(error) => progress.failure(&name, &error),
        }
    }

    let configs = subjects
        .iter()
        .map(|subject| subject.config.clone())
        .collect::<Vec<_>>();
    if update_managed {
        catalog::prepare(&configs).await?;
    }
    for subject in subjects {
        let name = subject.target.clone();
        let result = update_subject(
            &subject,
            mode,
            update_managed,
            !requested.is_empty(),
            &vendor_tools,
            update_all,
        )
        .await;
        match result {
            Ok(()) => progress.success(&name),
            Err(error) => progress.failure(&name, &error),
        }
    }
    progress.finish()
}

/// Apply the new global selection to running managed environments. Disabled
/// guest files remain in place; only currently selected tools are eligible for
/// subsequent updates.
pub(super) async fn reconcile_global_selection() -> VmResult<()> {
    let instances = vm_ops::resolve_fleet_targets(
        &FleetArgs {
            fleet: true,
            provider: None,
            pattern: None,
        },
        InstanceStateFilter::Running,
    )?;
    if instances.is_empty() {
        vm_println!("No running managed environments; the selection applies when one starts");
        return Ok(());
    }
    let mut progress = FleetProgress::default();
    for instance in instances {
        let name = instance.name.clone();
        let result: VmResult<()> = async {
            let subject = load_runtime_subject_for_instance(None, None, &instance)?;
            if !subject
                .provider
                .instance_state(Some(&name))
                .map_err(VmError::from)?
                .is_running()
            {
                return Ok(());
            }
            reconcile_environment(&subject)?;
            if !subject.config.tools.entries.is_empty() {
                catalog::prepare(std::slice::from_ref(&subject.config)).await?;
                apply_updates(
                    subject.provider.as_ref(),
                    &name,
                    &subject.config,
                    InstallMode::Wait,
                    false,
                )?;
            }
            Ok(())
        }
        .await;
        match result {
            Ok(()) => progress.success(&name),
            Err(error) => progress.failure(&name, &error),
        }
    }
    progress.finish()
}

fn partition_update_request(tools: Vec<String>) -> (Vec<String>, Vec<String>) {
    tools
        .into_iter()
        .partition(|name| base::is_vendor_tool(name))
}

fn remove_stale_vendor_selections(config: &mut VmConfig) {
    config
        .tools
        .entries
        .retain(|name, _| !base::is_vendor_tool(name));
}

async fn update_subject(
    subject: &RuntimeSubject,
    mode: InstallMode,
    update_managed: bool,
    managed_explicitly_selected: bool,
    vendor_tools: &[String],
    update_all_vendor_tools: bool,
) -> VmResult<()> {
    if !subject
        .provider
        .instance_state(Some(&subject.target))
        .map_err(VmError::from)?
        .is_running()
    {
        return Err(VmError::validation(
            format!(
                "Environment '{}' stopped before tool reconciliation",
                subject.target
            ),
            Some("Restart it to apply the selected tools"),
        ));
    }
    reconcile_environment(subject)?;
    if update_all_vendor_tools || !vendor_tools.is_empty() {
        base::update_vendor_tools(
            subject.provider.as_ref(),
            &subject.target,
            &subject.config,
            vendor_tools,
            update_all_vendor_tools,
            mode != InstallMode::Wait,
        )?;
    }
    if !update_managed || (managed_explicitly_selected && subject.config.tools.entries.is_empty()) {
        return Ok(());
    }
    apply_updates(
        subject.provider.as_ref(),
        &subject.target,
        &subject.config,
        mode,
        managed_explicitly_selected,
    )
}

pub(super) async fn activate_tool(subject: &mut RuntimeSubject, tool: &str) -> VmResult<()> {
    subject.config.tools.entries.retain(|name, _| name == tool);
    if subject.config.tools.entries.is_empty() {
        return Err(VmError::validation(
            format!("Tool '{tool}' is no longer enabled for this environment"),
            Some("Enable it globally or in the owning vm.yaml, then repair the rollout"),
        ));
    }
    catalog::prepare(std::slice::from_ref(&subject.config)).await?;
    update_subject(subject, InstallMode::Wait, true, true, &[], false).await
}

fn resolve_request(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    tools: &[String],
    environments: &[String],
    all_envs: bool,
    global: bool,
    include_stopped: bool,
) -> VmResult<(Vec<InstanceInfo>, Vec<String>)> {
    if global {
        return resolve_request_with(
            tools,
            environments,
            true,
            None,
            None,
            include_stopped,
            vm_ops::resolve_fleet_targets,
        );
    }
    let config = VmConfig::load(config_path.clone()).map_err(VmError::from)?;
    let project = vm_ops::FleetProject::new(config)?;
    let selected = if all_envs || !environments.is_empty() {
        None
    } else {
        Some(load_runtime_subject(config_path, profile, None)?.target)
    };
    resolve_request_with(
        tools,
        environments,
        all_envs,
        Some(&project.name),
        selected.as_deref(),
        include_stopped,
        |query, state| {
            vm_ops::filter_project_instances(vm_ops::resolve_fleet_targets(query, state)?, &project)
        },
    )
}

fn resolve_request_with(
    tools: &[String],
    environments: &[String],
    all_envs: bool,
    project: Option<&str>,
    selected: Option<&str>,
    include_stopped: bool,
    mut resolve: impl FnMut(&FleetArgs, InstanceStateFilter) -> VmResult<Vec<InstanceInfo>>,
) -> VmResult<(Vec<InstanceInfo>, Vec<String>)> {
    let query = FleetArgs {
        fleet: true,
        provider: None,
        pattern: None,
    };
    let instances = resolve(&query, InstanceStateFilter::Any)?
        .into_iter()
        .filter(|instance| {
            project.map_or(true, |project| instance.project.as_deref() == Some(project))
        })
        .collect::<Vec<_>>();
    let targets = if all_envs {
        instances
            .into_iter()
            .filter(|instance| include_stopped || vm_ops::is_running_status(&instance.status))
            .collect()
    } else {
        let names = if environments.is_empty() {
            vec![selected
                .ok_or_else(|| {
                    VmError::validation(
                        "No environment selected",
                        Some("Use --env NAME or --all-envs"),
                    )
                })?
                .to_string()]
        } else {
            environments.to_vec()
        };
        select_named_targets(instances, &names, include_stopped)?
    };
    if targets.is_empty() {
        if project.is_none() {
            return Ok((Vec::new(), tools.to_vec()));
        }
        return Err(VmError::validation(
            format!(
                "No matching running environments belong to {}",
                project.unwrap_or("the configured scope")
            ),
            Some("Run `vm list` to inspect project environments"),
        ));
    }
    Ok((targets, tools.to_vec()))
}

fn validate_configured_selection(
    requested: &BTreeSet<String>,
    configured: &BTreeSet<String>,
    load_failed: bool,
) -> VmResult<()> {
    let unconfigured = requested
        .difference(configured)
        .cloned()
        .collect::<Vec<_>>();
    if load_failed || unconfigured.is_empty() {
        return Ok(());
    }
    Err(VmError::validation(
        format!(
            "Selected tools are not configured in any targeted environment: {}",
            unconfigured.join(", ")
        ),
        Some(
            "Add each tool under `tools` in the project vm.yaml; select environments with `--env NAME`",
        ),
    ))
}

fn select_named_targets(
    instances: Vec<InstanceInfo>,
    requested: &[String],
    include_stopped: bool,
) -> VmResult<Vec<InstanceInfo>> {
    let mut available = instances
        .into_iter()
        .map(|instance| (instance.name.clone(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    let mut missing = Vec::new();
    for name in requested {
        match available.remove(name) {
            Some(instance) if include_stopped || vm_ops::is_running_status(&instance.status) => {
                selected.push(instance)
            }
            Some(_) => missing.push(name.clone()),
            None if !missing.contains(name) => missing.push(name.clone()),
            None => {}
        }
    }
    if !missing.is_empty() {
        return Err(VmError::validation(
            format!(
                "Managed environment{} not found{}: {}",
                if missing.len() == 1 { "" } else { "s" },
                if include_stopped {
                    ""
                } else {
                    " or not running"
                },
                missing.join(", ")
            ),
            Some(if include_stopped {
                "Use `vm list --all-projects`"
            } else {
                "Use `vm list --all-projects` or add --include-stopped"
            }),
        ));
    }
    Ok(selected)
}

fn select_configured_tools(config: &mut VmConfig, requested: &BTreeSet<String>) -> Vec<String> {
    if requested.is_empty() {
        return Vec::new();
    }
    config
        .tools
        .entries
        .retain(|name, _| requested.contains(name));
    config.tools.entries.keys().cloned().collect()
}

#[derive(Debug, Default)]
pub(super) struct UpdatePlan {
    pub(super) automatic: Vec<ToolArtifactRecord>,
    pub(super) prompt: Vec<ToolArtifactRecord>,
    pub(super) suppressed: Vec<ToolArtifactRecord>,
}

impl UpdatePlan {
    /// All changes allowed by an explicit update command. `Off` updates never
    /// enter either collection, while required installs and pin repairs do.
    pub(super) fn eligible(mut self) -> Vec<ToolArtifactRecord> {
        self.automatic.append(&mut self.prompt);
        self.automatic
    }
}

pub(super) fn plan(
    config: &VmConfig,
    available: &BTreeMap<String, ToolArtifactRecord>,
    installed: &BTreeMap<String, InstalledTool>,
    consumable: &BTreeMap<String, bool>,
) -> UpdatePlan {
    let mut plan = UpdatePlan::default();
    for (name, artifact) in available {
        let current = installed
            .get(name)
            .filter(|_| consumable.get(name).copied().unwrap_or(false));
        if current.is_some_and(|current| current.digest == artifact.artifact_digest) {
            continue;
        }
        let selection = &config.tools.entries[name];
        let change = artifact.clone();

        let is_install = current.is_none();
        let is_pin_reconciliation = !selection.tracks_latest();
        let is_newer = current.map_or(true, |current| {
            match (
                Version::parse(&current.version),
                Version::parse(&artifact.version),
            ) {
                (Ok(current), Ok(available)) => available > current,
                _ => true,
            }
        });
        if !is_install && !is_pin_reconciliation && !is_newer {
            continue;
        }

        if is_install || is_pin_reconciliation {
            plan.automatic.push(change);
            continue;
        }
        match selection.effective_updates(config.tools.updates) {
            ToolUpdatePolicy::Auto => plan.automatic.push(change),
            ToolUpdatePolicy::Prompt => plan.prompt.push(change),
            ToolUpdatePolicy::Off => plan.suppressed.push(change),
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vm_config::config::{ToolConfig, ToolsConfig};
    use vm_packages::ToolKind;
    use vm_provider::InstanceInfo;

    fn instance(name: &str, provider: &str) -> InstanceInfo {
        InstanceInfo {
            name: name.into(),
            id: format!("{name}-id"),
            status: "running".into(),
            provider: provider.into(),
            project: Some("demo".into()),
            uptime: None,
            created_at: None,
        }
    }

    fn artifact(name: &str, version: &str, digest: char) -> ToolArtifactRecord {
        ToolArtifactRecord {
            tool: name.into(),
            kind: ToolKind::Binary,
            version: version.into(),
            target: "linux-arm64".into(),
            artifact_digest: digest.to_string().repeat(64),
            size_bytes: 1,
            links: BTreeMap::from([(".local/bin/tool".into(), "bin/tool".into())]),
            source_repository: "https://example.com/tool.git".into(),
            source_commit: "f".repeat(40),
            tag: format!("v{version}"),
            artifact_path: "/tools/artifacts/tool".into(),
            actor: "release".into(),
            published_at: Utc::now(),
            receipt_id: "receipt".into(),
        }
    }

    fn config(policy: ToolUpdatePolicy, pinned: bool) -> VmConfig {
        let mut tools = ToolsConfig {
            updates: policy,
            ..Default::default()
        };
        tools.entries.insert(
            "codex".into(),
            ToolConfig {
                version: pinned.then(|| "2.0.0".into()),
                updates: None,
            },
        );
        VmConfig {
            tools,
            ..Default::default()
        }
    }

    #[test]
    fn update_request_separates_vm_owned_and_package_tools() {
        let (vendor, managed) = partition_update_request(vec![
            "agent-skills".into(),
            "codex".into(),
            "claude".into(),
            "typemill".into(),
            "antigravity".into(),
        ]);

        assert_eq!(vendor, ["codex", "claude", "antigravity"]);
        assert_eq!(managed, ["agent-skills", "typemill"]);
    }

    #[test]
    fn stale_vendor_entries_never_enter_the_package_catalog() {
        let mut config = VmConfig::default();
        config
            .tools
            .entries
            .insert("codex".into(), ToolConfig::default());
        config
            .tools
            .entries
            .insert("agent-skills".into(), ToolConfig::default());

        remove_stale_vendor_selections(&mut config);

        assert_eq!(
            config.tools.entries.keys().collect::<Vec<_>>(),
            ["agent-skills"]
        );
    }

    #[test]
    fn installs_and_pin_changes_are_automatic_but_latest_updates_follow_policy() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "2.0.0", 'b'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);

        let prompted = plan(
            &config(ToolUpdatePolicy::Prompt, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        );
        assert_eq!(prompted.prompt.len(), 1);
        let automatic = plan(
            &config(ToolUpdatePolicy::Off, true),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        );
        assert_eq!(automatic.automatic.len(), 1);
        let initial = plan(
            &config(ToolUpdatePolicy::Off, false),
            &available,
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(initial.automatic.len(), 1);
    }

    #[test]
    fn latest_never_downgrades_from_a_newer_guest_version() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "1.0.0", 'b'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "2.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);
        let plan = plan(
            &config(ToolUpdatePolicy::Auto, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        );
        assert!(plan.automatic.is_empty());
        assert!(plan.prompt.is_empty());
    }

    #[test]
    fn matching_but_non_consumable_release_is_reinstalled() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "1.0.0", 'a'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);

        let plan = plan(
            &config(ToolUpdatePolicy::Off, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), false)]),
        );

        assert_eq!(plan.automatic.len(), 1);
        assert!(plan.prompt.is_empty());
    }

    #[test]
    fn automatic_selection_never_includes_prompt_updates() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "2.0.0", 'b'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);

        let selected = plan(
            &config(ToolUpdatePolicy::Prompt, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        )
        .automatic;

        assert!(selected.is_empty());
    }

    #[test]
    fn explicit_update_selects_prompt_changes_but_respects_off() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "2.0.0", 'b'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);
        let consumable = BTreeMap::from([("codex".into(), true)]);

        let selected = plan(
            &config(ToolUpdatePolicy::Prompt, false),
            &available,
            &installed,
            &consumable,
        )
        .eligible();
        let disabled = plan(
            &config(ToolUpdatePolicy::Off, false),
            &available,
            &installed,
            &consumable,
        )
        .eligible();

        assert_eq!(selected.len(), 1);
        assert!(disabled.is_empty());
        let disabled = plan(
            &config(ToolUpdatePolicy::Off, false),
            &available,
            &installed,
            &consumable,
        );
        assert_eq!(disabled.suppressed.len(), 1);
    }

    #[test]
    fn explicit_selection_retains_only_configured_tools_and_their_pins() {
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

        let selected = select_configured_tools(
            &mut config,
            &BTreeSet::from(["agent-skills".into(), "unconfigured".into()]),
        );

        assert_eq!(selected, ["agent-skills"]);
        assert_eq!(
            config.tools.entries["agent-skills"].version.as_deref(),
            Some("0.8.0")
        );
        assert!(!config.tools.entries.contains_key("unconfigured"));
    }

    #[test]
    fn named_targets_preserve_order_across_providers_and_reject_missing() {
        let selected = select_named_targets(
            vec![instance("api-dev", "docker"), instance("mac", "tart")],
            &["mac".into(), "api-dev".into()],
            false,
        )
        .unwrap();
        assert_eq!(
            selected
                .into_iter()
                .map(|target| (target.name, target.provider))
                .collect::<Vec<_>>(),
            [
                ("mac".into(), "tart".into()),
                ("api-dev".into(), "docker".into())
            ]
        );
        let error = select_named_targets(
            vec![instance("api-dev", "docker")],
            &["missing".into()],
            true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("missing"));
        assert!(!error.to_string().contains("not running"));
    }

    #[test]
    fn targets_are_project_scoped_and_default_to_one_environment() {
        let (targets, tools) = resolve_request_with(
            &["agent-skills".into()],
            &["mac".into()],
            false,
            Some("demo"),
            None,
            false,
            |query, state| {
                assert_eq!(query.provider, None);
                assert_eq!(state, InstanceStateFilter::Any);
                Ok(vec![
                    instance("agent-skills-dev", "docker"),
                    instance("mac", "tart"),
                ])
            },
        )
        .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].provider, "tart");
        assert_eq!(tools, ["agent-skills"]);

        resolve_request_with(
            &["agent-skills".into()],
            &["mac".into()],
            false,
            Some("demo"),
            None,
            true,
            |_, state| {
                assert_eq!(state, InstanceStateFilter::Any);
                Ok(vec![instance("mac", "tart")])
            },
        )
        .unwrap();

        let (targets, tools) = resolve_request_with(
            &["agent-skills".into()],
            &[],
            false,
            Some("demo"),
            Some("agent-skills-dev"),
            false,
            |query, state| {
                assert_eq!(query.provider, None);
                assert_eq!(state, InstanceStateFilter::Any);
                Ok(vec![instance("agent-skills-dev", "docker")])
            },
        )
        .unwrap();
        assert_eq!(targets[0].name, "agent-skills-dev");
        assert_eq!(tools, ["agent-skills"]);

        let targets = resolve_request_with(&[], &[], true, Some("demo"), None, false, |_, _| {
            let mut unrelated = instance("other", "docker");
            unrelated.project = Some("other".into());
            Ok(vec![instance("mac", "tart"), unrelated])
        })
        .unwrap()
        .0;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "mac");
    }

    #[test]
    fn stopped_targets_require_explicit_deferral() {
        let mut stopped = instance("sleeping", "docker");
        stopped.status = "stopped".into();
        assert!(resolve_request_with(
            &[],
            &["sleeping".into()],
            false,
            Some("demo"),
            None,
            false,
            |_, _| Ok(vec![stopped.clone()])
        )
        .is_err());
        let selected = resolve_request_with(
            &[],
            &["sleeping".into()],
            false,
            Some("demo"),
            None,
            true,
            |_, _| Ok(vec![stopped.clone()]),
        )
        .unwrap()
        .0;
        assert_eq!(selected[0].name, "sleeping");
    }

    #[test]
    fn load_errors_preserve_selected_tool_validation() {
        let requested = BTreeSet::from(["agent-skills".into()]);
        assert!(validate_configured_selection(&requested, &BTreeSet::new(), true).is_ok());
        assert!(validate_configured_selection(&requested, &BTreeSet::new(), false).is_err());
    }
}
