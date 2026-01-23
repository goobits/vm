//! Fleet command handlers for cross-provider bulk operations

use std::collections::BTreeMap;

use tracing::{debug, info_span};

use crate::cli::{FleetSubcommand, FleetTargetArgs};
use crate::error::{VmError, VmResult};
use vm_core::vm_println;
use vm_provider::{get_provider, InstanceInfo, Provider};

use super::list::render_instance_table;
use super::targets::resolve_targets;

pub async fn handle_fleet_command(command: &FleetSubcommand, dry_run: bool) -> VmResult<()> {
    match command {
        FleetSubcommand::List { targets } => handle_list(targets),
        FleetSubcommand::Status { targets } => handle_status(targets),
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

fn handle_list(targets: &FleetTargetArgs) -> VmResult<()> {
    let instances = resolve_targets(
        targets.provider.as_deref(),
        targets.pattern.as_deref(),
        targets.running,
        targets.stopped,
    )?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    render_instance_table(instances);
    Ok(())
}

fn handle_status(targets: &FleetTargetArgs) -> VmResult<()> {
    let instances = resolve_targets(
        targets.provider.as_deref(),
        targets.pattern.as_deref(),
        targets.running,
        targets.stopped,
    )?;

    if instances.is_empty() {
        vm_println!("No instances found");
        return Ok(());
    }

    render_instance_table(instances);
    Ok(())
}

fn handle_exec(targets: &FleetTargetArgs, command: &[String], dry_run: bool) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_exec");
    let _enter = span.enter();

    let instances = resolve_targets(
        targets.provider.as_deref(),
        targets.pattern.as_deref(),
        targets.running,
        targets.stopped,
    )?;

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

    for (provider_name, provider_instances) in group_by_provider(instances)? {
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

    summary(success, failed);
    Ok(())
}

fn handle_copy(
    targets: &FleetTargetArgs,
    source: &str,
    destination: &str,
    dry_run: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_copy");
    let _enter = span.enter();

    let instances = resolve_targets(
        targets.provider.as_deref(),
        targets.pattern.as_deref(),
        targets.running,
        targets.stopped,
    )?;

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

    for (provider_name, provider_instances) in group_by_provider(instances)? {
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

    summary(success, failed);
    Ok(())
}

enum Action {
    Start,
    Stop,
    Restart,
}

fn handle_start_stop(targets: &FleetTargetArgs, action: Action, dry_run: bool) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "fleet_lifecycle");
    let _enter = span.enter();

    let instances = resolve_targets(
        targets.provider.as_deref(),
        targets.pattern.as_deref(),
        targets.running,
        targets.stopped,
    )?;

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

    for (provider_name, provider_instances) in group_by_provider(instances)? {
        let provider = provider_for(&provider_name)?;
        for instance in provider_instances {
            let result = match action {
                Action::Start => provider.start(Some(&instance.name)),
                Action::Stop => provider.stop(Some(&instance.name)),
                Action::Restart => provider.restart(Some(&instance.name)),
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

    summary(success, failed);
    Ok(())
}

fn provider_for(provider_name: &str) -> VmResult<Box<dyn Provider>> {
    use vm_config::config::VmConfig;

    let config = VmConfig {
        provider: Some(provider_name.to_string()),
        ..Default::default()
    };
    get_provider(config).map_err(VmError::from)
}

fn group_by_provider(
    instances: Vec<InstanceInfo>,
) -> VmResult<BTreeMap<String, Vec<InstanceInfo>>> {
    let mut grouped: BTreeMap<String, Vec<InstanceInfo>> = BTreeMap::new();
    for instance in instances {
        grouped
            .entry(instance.provider.clone())
            .or_default()
            .push(instance);
    }
    Ok(grouped)
}

fn summary(success: usize, failed: usize) {
    let total = success + failed;
    if failed == 0 {
        vm_println!("\n✓ {} of {} succeeded", success, total);
    } else {
        vm_println!("\n✓ {} of {} succeeded, {} failed", success, total, failed);
    }
}
