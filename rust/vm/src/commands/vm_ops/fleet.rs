//! Shared cross-provider targeting for `--fleet` operations.

use std::collections::BTreeMap;

use tracing::{debug, info_span};

use crate::cli::FleetArgs;
use crate::error::{VmError, VmResult};
use vm_config::config::VmConfig;
use vm_core::{vm_println, vm_success, vm_warning};
use vm_provider::{get_provider, InstanceInfo, Provider, ProviderContext};

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

pub(in crate::commands) fn configured_provider(
    config: &VmConfig,
    provider_name: &str,
) -> VmResult<Box<dyn Provider>> {
    let mut config = config.clone();
    config.provider = Some(provider_name.to_string());
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

pub fn handle_fleet_exec(targets: &FleetArgs, command: &[String]) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_exec");
    let _enter = span.enter();

    let instances = resolve_fleet_targets(targets, InstanceStateFilter::Running)?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    let mut progress = FleetProgress::default();

    for (provider_name, provider_instances) in group_by_provider(instances) {
        let provider = provider_for(&provider_name)?;
        for instance in provider_instances {
            debug!(
                "Fleet exec: provider={}, instance={}, command={:?}",
                provider_name, instance.name, command
            );
            match provider.exec(Some(&instance.name), command) {
                Ok(()) => {
                    progress.success(&instance.name);
                }
                Err(e) => {
                    progress.failure(&instance.name, &e);
                }
            }
        }
    }

    progress.finish()
}

pub fn handle_fleet_copy(targets: &FleetArgs, source: &str, destination: &str) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_copy");
    let _enter = span.enter();

    let instances = resolve_fleet_targets(targets, InstanceStateFilter::Running)?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    let mut progress = FleetProgress::default();

    for (provider_name, provider_instances) in group_by_provider(instances) {
        let provider = provider_for(&provider_name)?;
        for instance in provider_instances {
            debug!(
                "Fleet copy: provider={}, instance={}, source={}, destination={}",
                provider_name, instance.name, source, destination
            );
            match provider.copy(source, destination, Some(&instance.name)) {
                Ok(()) => {
                    progress.success(&instance.name);
                }
                Err(e) => {
                    progress.failure(&instance.name, &e);
                }
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
    action: FleetAction,
    no_wait: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_lifecycle");
    let _enter = span.enter();

    let default_state = match action {
        FleetAction::Start => InstanceStateFilter::Stopped,
        FleetAction::Stop | FleetAction::Restart => InstanceStateFilter::Running,
    };
    let instances = resolve_fleet_targets(targets, default_state)?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    let mut progress = FleetProgress::default();
    let context = ProviderContext::default();

    for (provider_name, provider_instances) in group_by_provider(instances) {
        let provider = provider_for(&provider_name)?;
        for instance in provider_instances {
            let result = match action {
                FleetAction::Start => provider.start(Some(&instance.name), &context),
                FleetAction::Stop => provider.stop(Some(&instance.name)),
                FleetAction::Restart => provider.restart(Some(&instance.name), &context),
            };

            match result {
                Ok(()) => {
                    let should_wait = match action {
                        FleetAction::Start => !no_wait,
                        FleetAction::Restart => true,
                        FleetAction::Stop => false,
                    };
                    if should_wait {
                        match wait_until_commands_ready(
                            provider.as_ref(),
                            Some(&instance.name),
                            &instance.name,
                        )
                        .await
                        {
                            Ok(()) => {
                                progress.success(&instance.name);
                            }
                            Err(error) => {
                                progress.failure(&instance.name, &error);
                            }
                        }
                    } else {
                        progress.success(&instance.name);
                    }
                }
                Err(e) => {
                    progress.failure(&instance.name, &e);
                }
            }
        }
    }

    progress.finish()
}

fn provider_for(provider_name: &str) -> VmResult<Box<dyn Provider>> {
    configured_provider(&VmConfig::default(), provider_name)
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
    use super::{query_for, InstanceStateFilter};
    use crate::cli::FleetArgs;

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
}
