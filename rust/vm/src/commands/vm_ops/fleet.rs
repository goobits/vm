//! Fleet command handlers for cross-provider bulk operations

use std::collections::BTreeMap;

use tracing::{debug, info_span};

use crate::cli::{FleetSubcommand, FleetTargetArgs};
use crate::error::{VmError, VmResult};
use vm_core::vm_println;
use vm_provider::{get_provider, InstanceInfo, Provider, ProviderContext};

use super::targets::{resolve_targets, InstanceStateFilter, TargetQuery};

pub async fn handle_fleet_command(command: &FleetSubcommand, dry_run: bool) -> VmResult<()> {
    match command {
        FleetSubcommand::Exec { targets, command } => handle_exec(targets, command, dry_run),
        FleetSubcommand::Copy {
            targets,
            source,
            destination,
        } => handle_copy(targets, source, destination, dry_run),
        FleetSubcommand::Start { targets } => handle_start_stop(targets, Action::Start, dry_run),
        FleetSubcommand::Stop { targets } => handle_start_stop(targets, Action::Stop, dry_run),
        FleetSubcommand::Restart { targets } => {
            handle_start_stop(targets, Action::Restart, dry_run)
        }
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

fn handle_exec(targets: &FleetTargetArgs, command: &[String], dry_run: bool) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_exec");
    let _enter = span.enter();

    let instances = resolve_targets(query_for(targets, InstanceStateFilter::Running))?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    let cmd_display = command.join(" ");
    if dry_run {
        vm_println!(
            "Dry run: Would execute `{}` on {} instances",
            cmd_display,
            instances.len()
        );
        for instance in &instances {
            vm_println!("  - {} ({})", instance.name, instance.provider);
        }
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
                    vm_println!("  ✓ {}", instance.name);
                    success += 1;
                }
                Err(e) => {
                    vm_println!("  ✗ {}: {}", instance.name, e);
                    failed += 1;
                }
            }
        }
    }

    summary(success, failed)
}

fn handle_copy(
    targets: &FleetTargetArgs,
    source: &str,
    destination: &str,
    dry_run: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_copy");
    let _enter = span.enter();

    let instances = resolve_targets(query_for(targets, InstanceStateFilter::Running))?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    if dry_run {
        vm_println!(
            "Dry run: Would copy {} -> {} on {} instances",
            source,
            destination,
            instances.len()
        );
        for instance in &instances {
            vm_println!("  - {} ({})", instance.name, instance.provider);
        }
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
                    vm_println!("  ✓ {}", instance.name);
                    success += 1;
                }
                Err(e) => {
                    vm_println!("  ✗ {}: {}", instance.name, e);
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

fn handle_start_stop(targets: &FleetTargetArgs, action: Action, dry_run: bool) -> VmResult<()> {
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

    let action_label = match action {
        Action::Start => "start",
        Action::Stop => "stop",
        Action::Restart => "restart",
    };

    if dry_run {
        vm_println!(
            "Dry run: Would {} {} instances",
            action_label,
            instances.len()
        );
        for instance in &instances {
            vm_println!("  - {} ({})", instance.name, instance.provider);
        }
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
                    vm_println!("  ✓ {}", instance.name);
                    success += 1;
                }
                Err(e) => {
                    vm_println!("  ✗ {}: {}", instance.name, e);
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
        vm_println!("\n✓ {} of {} succeeded", success, total);
    } else {
        vm_println!("\n✓ {} of {} succeeded, {} failed", success, total, failed);
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
