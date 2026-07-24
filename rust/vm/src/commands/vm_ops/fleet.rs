//! Fleet command handlers for cross-provider bulk operations

use std::collections::BTreeMap;

use tracing::{debug, info_span};

use crate::cli::{FleetSubcommand, FleetTargetArgs};
use crate::error::{VmError, VmResult};
use vm_core::{vm_println, vm_success, vm_warning};
use vm_provider::{get_provider, InstanceInfo, Provider, ProviderContext};

use super::targets::{resolve_targets, InstanceStateFilter, TargetQuery};

pub async fn handle_fleet_command(command: &FleetSubcommand) -> VmResult<()> {
    match command {
        FleetSubcommand::Exec { targets, command } => handle_exec(targets, command),
        FleetSubcommand::Copy {
            targets,
            source,
            destination,
        } => handle_copy(targets, source, destination),
        FleetSubcommand::Start { targets } => handle_start_stop(targets, Action::Start),
        FleetSubcommand::Stop { targets } => handle_start_stop(targets, Action::Stop),
        FleetSubcommand::Restart { targets } => handle_start_stop(targets, Action::Restart),
    }
}

fn query_for(targets: &FleetTargetArgs, default_state: InstanceStateFilter) -> TargetQuery<'_> {
    let state = if targets.running {
        InstanceStateFilter::Running
    } else if targets.stopped {
        InstanceStateFilter::Stopped
    } else {
        default_state
    };

    TargetQuery {
        provider: targets.provider.as_deref(),
        pattern: targets.pattern.as_deref(),
        state,
    }
}

fn handle_exec(targets: &FleetTargetArgs, command: &[String]) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_exec");
    let _enter = span.enter();

    let instances = resolve_targets(query_for(targets, InstanceStateFilter::Running))?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    let mut success = 0;
    let mut failed = 0;

    for (provider_name, provider_instances) in group_by_provider(instances) {
        let provider = provider_for(&provider_name)?;
        for instance in provider_instances {
            debug!(
                "Fleet exec: provider={}, instance={}, command={:?}",
                provider_name, instance.name, command
            );
            match provider.exec(Some(&instance.name), command) {
                Ok(()) => {
                    vm_success!("{}", instance.name);
                    success += 1;
                }
                Err(e) => {
                    vm_warning!("{}: {}", instance.name, e);
                    failed += 1;
                }
            }
        }
    }

    summary(success, failed)
}

fn handle_copy(targets: &FleetTargetArgs, source: &str, destination: &str) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_copy");
    let _enter = span.enter();

    let instances = resolve_targets(query_for(targets, InstanceStateFilter::Running))?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    let mut success = 0;
    let mut failed = 0;

    for (provider_name, provider_instances) in group_by_provider(instances) {
        let provider = provider_for(&provider_name)?;
        for instance in provider_instances {
            debug!(
                "Fleet copy: provider={}, instance={}, source={}, destination={}",
                provider_name, instance.name, source, destination
            );
            match provider.copy(source, destination, Some(&instance.name)) {
                Ok(()) => {
                    vm_success!("{}", instance.name);
                    success += 1;
                }
                Err(e) => {
                    vm_warning!("{}: {}", instance.name, e);
                    failed += 1;
                }
            }
        }
    }

    summary(success, failed)
}

enum Action {
    Start,
    Stop,
    Restart,
}

fn handle_start_stop(targets: &FleetTargetArgs, action: Action) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_lifecycle");
    let _enter = span.enter();

    let default_state = match action {
        Action::Start => InstanceStateFilter::Stopped,
        Action::Stop | Action::Restart => InstanceStateFilter::Running,
    };
    let instances = resolve_targets(query_for(targets, default_state))?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    let mut success = 0;
    let mut failed = 0;
    let context = ProviderContext::default();

    for (provider_name, provider_instances) in group_by_provider(instances) {
        let provider = provider_for(&provider_name)?;
        for instance in provider_instances {
            let result = match action {
                Action::Start => provider.start(Some(&instance.name), &context),
                Action::Stop => provider.stop(Some(&instance.name)),
                Action::Restart => provider.restart(Some(&instance.name), &context),
            };

            match result {
                Ok(()) => {
                    vm_success!("{}", instance.name);
                    success += 1;
                }
                Err(e) => {
                    vm_warning!("{}: {}", instance.name, e);
                    failed += 1;
                }
            }
        }
    }

    summary(success, failed)
}

fn provider_for(provider_name: &str) -> VmResult<Box<dyn Provider>> {
    use vm_config::config::VmConfig;

    let config = VmConfig {
        provider: Some(provider_name.to_string()),
        ..Default::default()
    };
    get_provider(config).map_err(VmError::from)
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
    use crate::cli::FleetTargetArgs;

    fn targets() -> FleetTargetArgs {
        FleetTargetArgs {
            provider: None,
            pattern: None,
            running: false,
            stopped: false,
        }
    }

    #[test]
    fn query_uses_command_default_when_no_state_filter_is_supplied() {
        let targets = targets();
        let query = query_for(&targets, InstanceStateFilter::Running);

        assert_eq!(query.state, InstanceStateFilter::Running);
    }

    #[test]
    fn explicit_state_filter_overrides_command_default() {
        let mut targets = targets();
        targets.stopped = true;

        let query = query_for(&targets, InstanceStateFilter::Running);

        assert_eq!(query.state, InstanceStateFilter::Stopped);
    }
}
