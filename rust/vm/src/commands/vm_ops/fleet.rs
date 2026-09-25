//! Shared project-scoped targeting for `--all-envs` operations.

use std::collections::BTreeMap;
use std::path::PathBuf;

use tracing::{debug, info_span};

use crate::cli::{ExecOutput, FleetArgs};
use crate::commands::command_context::{project_name, require_project_config};
use crate::commands::status;
use crate::error::{VmError, VmResult};
use vm_config::config::VmConfig;
use vm_core::{vm_println, vm_success, vm_warning};
use vm_provider::{
    get_provider, ExecOptions, InstanceInfo, InstanceProvider, InstanceState, Provider,
    ProviderContext,
};

use super::{
    lifecycle::wait_until_commands_ready,
    targets::{resolve_targets, InstanceStateFilter, TargetQuery},
};

fn query_for(targets: &FleetArgs, state: InstanceStateFilter) -> TargetQuery<'_> {
    TargetQuery {
        provider: targets.provider.as_deref(),
        pattern: targets.pattern.as_deref(),
        state,
    }
}

pub(in crate::commands) fn resolve_fleet_targets(
    targets: &FleetArgs,
    state: InstanceStateFilter,
) -> VmResult<Vec<InstanceInfo>> {
    resolve_targets(query_for(targets, state))
}

pub struct FleetProject {
    pub name: String,
    pub config_path: PathBuf,
    pub config: VmConfig,
}

impl FleetProject {
    pub fn new(config: VmConfig) -> VmResult<Self> {
        require_project_config(&config)?;
        let config_path = config
            .owning_config_path()
            .ok_or_else(|| {
                VmError::validation("Project configuration has no source path", None::<String>)
            })?
            .canonicalize()
            .map_err(VmError::from)?;
        Ok(Self {
            name: project_name(&config).to_string(),
            config_path,
            config,
        })
    }
}

pub(in crate::commands) fn project_targets(
    targets: &FleetArgs,
    state: InstanceStateFilter,
    project: &FleetProject,
) -> VmResult<Vec<InstanceInfo>> {
    let instances = filter_project_instances(resolve_fleet_targets(targets, state)?, project)?;
    if instances.is_empty() {
        return Err(VmError::validation(
            format!(
                "No matching environments belong to project '{}'",
                project.name
            ),
            Some("Run `vm list` to inspect project environments"),
        ));
    }
    Ok(instances)
}

pub(in crate::commands) fn filter_project_instances(
    instances: Vec<InstanceInfo>,
    project: &FleetProject,
) -> VmResult<Vec<InstanceInfo>> {
    filter_project_instances_with(instances, project, |instance| {
        let provider = configured_provider(&project.config, &instance.provider)?;
        provider
            .instance_config_path(&instance.name)
            .map_err(VmError::from)
    })
}

fn filter_project_instances_with(
    instances: Vec<InstanceInfo>,
    project: &FleetProject,
    mut owner: impl FnMut(&InstanceInfo) -> VmResult<Option<PathBuf>>,
) -> VmResult<Vec<InstanceInfo>> {
    let mut selected = Vec::new();
    for instance in instances {
        if instance.project.as_deref() != Some(&project.name) {
            continue;
        }
        if same_owner(owner(&instance)?.as_deref(), &project.config_path) {
            selected.push(instance);
        }
    }
    Ok(selected)
}

fn same_owner(owner: Option<&std::path::Path>, selected: &std::path::Path) -> bool {
    owner.and_then(|path| path.canonicalize().ok()).as_deref() == Some(selected)
}

pub(in crate::commands) fn configured_provider(
    config: &VmConfig,
    provider_name: &str,
) -> VmResult<Box<dyn Provider>> {
    let mut config = config.clone();
    config.provider = Some(provider_name.into());
    get_provider(config).map_err(VmError::from)
}

#[derive(Debug, Default)]
pub(in crate::commands) struct FleetProgress {
    succeeded: usize,
    failed: usize,
}

impl FleetProgress {
    pub(in crate::commands) fn success(&mut self, name: &str) {
        vm_success!("{name}");
        self.succeeded += 1;
    }

    pub(in crate::commands) fn failure(&mut self, name: &str, error: &dyn std::fmt::Display) {
        vm_warning!("{name}: {error}");
        self.failed += 1;
    }

    pub(in crate::commands) fn finish(self) -> VmResult<()> {
        summary(self.succeeded, self.failed)
    }
}

pub fn handle_fleet_exec(
    targets: &FleetArgs,
    project: &FleetProject,
    command: &[String],
    options: &ExecOptions,
    output: ExecOutput,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_exec");
    let _enter = span.enter();

    let instances = project_targets(targets, InstanceStateFilter::Running, project)?;

    let targets = instances
        .into_iter()
        .map(|instance| {
            debug!(
                provider = %instance.provider,
                instance = %instance.name,
                argument_count = command.len(),
                "Executing fleet command"
            );
            super::fleet_exec::ExecTarget {
                provider: configured_provider_for_instance(project, &instance),
                name: instance.name,
            }
        })
        .collect();
    super::fleet_exec::run(targets, command, options, output)
}

pub fn handle_fleet_status(targets: &FleetArgs, project: &FleetProject) -> VmResult<()> {
    let instances = project_targets(targets, InstanceStateFilter::Any, project)?;
    let mut progress = FleetProgress::default();
    for (_provider_name, provider_instances) in group_by_provider(instances) {
        for instance in provider_instances {
            let outcome = (|| {
                let provider = configured_provider_for_instance(project, &instance)?;
                let report = provider
                    .status(Some(&instance.name))
                    .map_err(VmError::from)?;
                status::display(&report);
                display_drift(provider.as_ref(), &instance.name)
            })();
            match outcome {
                Ok(()) => progress.success(&instance.name),
                Err(error) => progress.failure(&instance.name, &error),
            }
        }
    }
    progress.finish()
}

#[derive(Debug, Clone, Copy)]
pub enum FleetAction {
    Start,
    Stop,
    Restart,
}

pub async fn handle_fleet_lifecycle(
    targets: &FleetArgs,
    project: &FleetProject,
    action: FleetAction,
    no_wait: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_lifecycle");
    let _enter = span.enter();

    let instances = project_targets(targets, InstanceStateFilter::Any, project)?;

    let mut progress = FleetProgress::default();
    let context = ProviderContext::default();

    for (_provider_name, provider_instances) in group_by_provider(instances) {
        for instance in provider_instances {
            let outcome = async {
                let provider = configured_provider_for_instance(project, &instance)?;
                apply_lifecycle(provider.as_ref(), &context, &instance.name, action, no_wait).await
            }
            .await;
            match outcome {
                Ok(()) => progress.success(&instance.name),
                Err(error) => progress.failure(&instance.name, &error),
            }
        }
    }

    progress.finish()
}

pub(super) async fn apply_lifecycle(
    provider: &dyn InstanceProvider,
    context: &ProviderContext,
    environment: &str,
    action: FleetAction,
    no_wait: bool,
) -> VmResult<()> {
    if matches!(action, FleetAction::Start | FleetAction::Restart) {
        if let Some(drift) = provider.runtime_drift(environment).map_err(VmError::from)? {
            return Err(VmError::validation(
                format!("Configuration drift for '{environment}': {drift}"),
                Some("Review the changed settings, then remove and recreate this environment"),
            ));
        }
    }
    let state = provider
        .instance_state(Some(environment))
        .map_err(VmError::from)?;
    match (action, state) {
        (FleetAction::Start, InstanceState::Running | InstanceState::Starting)
        | (
            FleetAction::Stop,
            InstanceState::Stopped | InstanceState::Paused | InstanceState::Suspended,
        ) => {}
        (
            FleetAction::Start,
            InstanceState::Stopped | InstanceState::Paused | InstanceState::Suspended,
        )
        | (
            FleetAction::Restart,
            InstanceState::Stopped | InstanceState::Paused | InstanceState::Suspended,
        ) => {
            provider
                .start(Some(environment), context)
                .map_err(VmError::from)?;
        }
        (FleetAction::Stop, InstanceState::Running | InstanceState::Starting) => {
            provider.stop(Some(environment)).map_err(VmError::from)?;
        }
        (FleetAction::Restart, InstanceState::Running | InstanceState::Starting) => {
            provider
                .restart(Some(environment), context)
                .map_err(VmError::from)?;
        }
        (_, InstanceState::Unknown(state)) => {
            return Err(VmError::validation(
                format!("Cannot apply lifecycle action to '{environment}' in state '{state}'"),
                Some("Run `vm status` for details"),
            ))
        }
    }

    let should_wait = match action {
        FleetAction::Start => !no_wait,
        FleetAction::Restart => true,
        FleetAction::Stop => false,
    };
    if should_wait {
        wait_until_commands_ready(provider, Some(environment), environment).await?;
    }
    Ok(())
}

pub(in crate::commands) fn configured_provider_for_instance(
    project: &FleetProject,
    instance: &InstanceInfo,
) -> VmResult<Box<dyn Provider>> {
    let selected = selected_config_for_instance(project, instance);
    configured_provider(&selected, &instance.provider)
}

pub(super) fn selected_config_for_instance(
    project: &FleetProject,
    instance: &InstanceInfo,
) -> VmConfig {
    project
        .config
        .environments
        .iter()
        .find_map(|(name, declaration)| {
            (declaration.provider.as_str() == instance.provider
                && super::target::canonical_instance_name(
                    &instance.provider,
                    &project.name,
                    Some(name),
                ) == instance.name)
                .then(|| declaration.apply_to(&project.config))
        })
        .unwrap_or_else(|| project.config.clone())
}

fn display_drift(provider: &dyn InstanceProvider, instance: &str) -> VmResult<()> {
    if !provider.supports_runtime_drift_detection() {
        vm_println!("Configuration: unknown (provider has no drift record)");
        return Ok(());
    }
    match provider.runtime_drift(instance).map_err(VmError::from)? {
        Some(reason) => vm_println!("Configuration drift: {reason}"),
        None => vm_println!("Configuration: current"),
    }
    Ok(())
}

fn group_by_provider(instances: Vec<InstanceInfo>) -> BTreeMap<String, Vec<InstanceInfo>> {
    let mut grouped: BTreeMap<String, Vec<InstanceInfo>> = BTreeMap::new();
    for instance in instances {
        grouped
            .entry(instance.provider.clone())
            .or_default()
            .push(instance);
    }
    grouped
}

fn summary(success: usize, failed: usize) -> VmResult<()> {
    let total = success + failed;
    if failed == 0 {
        vm_success!("{} of {} succeeded", success, total);
    } else {
        vm_println!("{} of {} succeeded; {} failed", success, total, failed);
        return Err(VmError::general(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                "one or more fleet operations failed",
            ),
            format!("{failed} of {total} fleet operations failed"),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        filter_project_instances_with, query_for, same_owner, selected_config_for_instance,
        FleetProject, InstanceStateFilter,
    };
    use crate::cli::FleetArgs;
    use std::fs;
    use vm_config::config::VmConfig;
    use vm_provider::InstanceInfo;

    fn targets() -> FleetArgs {
        FleetArgs {
            fleet: true,
            provider: None,
            pattern: None,
        }
    }

    #[test]
    fn query_uses_command_default_when_no_state_filter_is_supplied() {
        let targets = targets();
        let query = query_for(&targets, InstanceStateFilter::Running);

        assert_eq!(query.state, InstanceStateFilter::Running);
    }

    #[test]
    fn query_uses_explicit_provider_and_pattern_filters() {
        let mut targets = targets();
        targets.provider = Some("docker".into());
        targets.pattern = Some("app-*".into());

        let query = query_for(&targets, InstanceStateFilter::Running);

        assert_eq!(query.provider, Some("docker"));
        assert_eq!(query.pattern, Some("app-*"));
    }

    #[test]
    fn project_inventory_and_fleet_require_the_exact_owning_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let selected = temp.path().join("selected.yaml");
        let other = temp.path().join("other.yaml");
        fs::write(&selected, "project: app").unwrap();
        fs::write(&other, "project: app").unwrap();
        let selected = selected.canonicalize().unwrap();
        assert!(same_owner(Some(&selected), &selected));
        assert!(!same_owner(Some(&other), &selected));
        assert!(!same_owner(None, &selected));

        let project = FleetProject {
            name: "app".into(),
            config_path: selected.clone(),
            config: VmConfig::default(),
        };
        let instance = |name: &str, project: &str| InstanceInfo {
            name: name.into(),
            id: name.into(),
            status: "running".into(),
            provider: "docker".into(),
            project: Some(project.into()),
            uptime: None,
            created_at: None,
        };
        let instances = vec![
            instance("owned", "app"),
            instance("same-name-other-config", "app"),
            instance("other-project", "other"),
        ];
        let actual = filter_project_instances_with(instances, &project, |instance| {
            Ok(Some(if instance.name == "owned" {
                selected.clone()
            } else {
                other.clone()
            }))
        })
        .unwrap();
        assert_eq!(actual.len(), 1);
        assert_eq!(actual[0].name, "owned");
    }

    #[test]
    fn instance_selection_applies_its_declared_runtime_config() {
        let config: VmConfig = serde_yaml_ng::from_str(
            "project:\n  name: demo\nvm:\n  image: ubuntu:22.04\nenvironments:\n  test:\n    provider: docker\n    image: ubuntu:24.04\n",
        ).unwrap();
        let project = FleetProject {
            name: "demo".into(),
            config_path: "/tmp/demo/vm.yaml".into(),
            config,
        };
        let instance = InstanceInfo {
            name: "demo-test-dev".into(),
            id: "id".into(),
            status: "stopped".into(),
            provider: "docker".into(),
            project: Some("demo".into()),
            uptime: None,
            created_at: None,
        };
        let selected = selected_config_for_instance(&project, &instance);
        let image = selected.vm.unwrap().image.unwrap();
        assert!(format!("{image:?}").contains("24.04"));
    }
}
