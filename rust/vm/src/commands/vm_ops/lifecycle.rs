//! Lifecycle handlers for existing environments.

use std::time::Duration;

use tracing::{debug, info_span};

use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_hint, vm_progress, vm_success};
use vm_provider::{InstanceState, Provider, ProviderContext};

use super::helpers::{
    print_vm_runtime_details, register_vm_services_helper, unregister_vm_services_helper,
};

const READY_ATTEMPTS: usize = 120;
const READY_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StartOutcome {
    AlreadyRunning,
    Started,
}

fn default_resource_name(provider: &dyn Provider, vm_name: &str) -> String {
    match provider.name() {
        "tart" => vm_name.to_string(),
        _ => format!("{vm_name}-dev"),
    }
}

fn project_name(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project")
}

fn target_name(provider: &dyn Provider, container: Option<&str>, config: &VmConfig) -> String {
    container
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_resource_name(provider, project_name(config)))
}

async fn wait_until_ready(
    provider: &dyn Provider,
    container: Option<&str>,
    display_name: &str,
) -> VmResult<()> {
    let mut announced = false;
    let mut last_error = None;

    for attempt in 0..READY_ATTEMPTS {
        match provider.is_ready(container) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => last_error = Some(error),
        }

        if !announced {
            vm_progress!("Waiting for '{display_name}' to be ready...");
            announced = true;
        }
        if attempt + 1 < READY_ATTEMPTS {
            tokio::time::sleep(READY_INTERVAL).await;
        }
    }

    let source = last_error.unwrap_or_else(|| {
        vm_core::error::VmError::Timeout(format!(
            "Environment '{display_name}' did not become ready within 60 seconds"
        ))
    });
    Err(VmError::vm_operation(
        source,
        Some(display_name),
        "wait until ready",
    ))
}

/// Start an existing environment when needed, then wait until it accepts commands.
///
/// This function never creates, rebuilds, or removes an environment.
pub(super) async fn ensure_running(
    provider: &dyn Provider,
    container: Option<&str>,
    config: &VmConfig,
    global_config: &GlobalConfig,
    wait: bool,
) -> VmResult<StartOutcome> {
    let display_name = target_name(provider, container, config);
    let state = provider.instance_state(container).map_err(VmError::from)?;

    let should_start = match state {
        InstanceState::Running => false,
        InstanceState::Starting => {
            vm_progress!("'{display_name}' is starting...");
            true
        }
        InstanceState::Stopped | InstanceState::Paused | InstanceState::Suspended => {
            vm_progress!("Starting '{display_name}'...");
            let context = ProviderContext::default().with_config(global_config.clone());
            if let Err(start_error) = provider.start(container, &context) {
                match provider.instance_state(container) {
                    Ok(InstanceState::Running | InstanceState::Starting) => {}
                    _ => return Err(VmError::from(start_error)),
                }
            }
            true
        }
        InstanceState::Unknown(state) => {
            return Err(VmError::validation(
                format!("Cannot start '{display_name}' from state '{state}'"),
                Some("Run `vm status` for details"),
            ));
        }
    };

    if wait {
        wait_until_ready(provider, container, &display_name).await?;
    }

    if !should_start {
        return Ok(StartOutcome::AlreadyRunning);
    }

    vm_success!("Started '{display_name}'");
    if config.services.values().any(|service| service.enabled) {
        vm_progress!("Configuring services...");
        register_vm_services_helper(&display_name, config, global_config).await?;
    }
    Ok(StartOutcome::Started)
}

/// Handle `vm start`.
pub async fn handle_start(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
    no_wait: bool,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "start");
    let _enter = span.enter();
    let display_name = target_name(provider.as_ref(), container, &config);
    debug!(target = %display_name, "Starting environment");

    match ensure_running(
        provider.as_ref(),
        container,
        &config,
        &global_config,
        !no_wait,
    )
    .await?
    {
        StartOutcome::AlreadyRunning => vm_success!("'{display_name}' is already running"),
        StartOutcome::Started => print_vm_runtime_details(&config, false),
    }
    vm_hint!("Connect with: vm shell {display_name}");
    Ok(())
}

/// Handle a graceful environment stop.
pub async fn handle_stop(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "stop");
    let _enter = span.enter();
    let display_name = target_name(provider.as_ref(), container, &config);
    debug!(target = %display_name, "Stopping environment");

    vm_progress!("Stopping '{display_name}'...");
    provider.stop(container).map_err(VmError::from)?;
    if config.services.values().any(|service| service.enabled) {
        unregister_vm_services_helper(&display_name, &global_config).await?;
    }
    vm_success!("Stopped '{display_name}'");
    Ok(())
}

/// Handle `vm restart`; stopped environments are started.
pub async fn handle_restart(
    provider: Box<dyn Provider>,
    container: Option<&str>,
    config: VmConfig,
    global_config: GlobalConfig,
) -> VmResult<()> {
    let span = info_span!("vm_operation", operation = "restart");
    let _enter = span.enter();
    let display_name = target_name(provider.as_ref(), container, &config);
    debug!(target = %display_name, "Restarting environment");

    vm_progress!("Restarting '{display_name}'...");
    let context = ProviderContext::default().with_config(global_config.clone());
    match provider.instance_state(container).map_err(VmError::from)? {
        InstanceState::Running | InstanceState::Starting => provider
            .restart(container, &context)
            .map_err(VmError::from)?,
        InstanceState::Stopped | InstanceState::Paused | InstanceState::Suspended => {
            provider.start(container, &context).map_err(VmError::from)?;
        }
        InstanceState::Unknown(state) => {
            return Err(VmError::validation(
                format!("Cannot restart '{display_name}' from state '{state}'"),
                Some("Run `vm status` for details"),
            ));
        }
    }

    wait_until_ready(provider.as_ref(), container, &display_name).await?;
    if config.services.values().any(|service| service.enabled) {
        register_vm_services_helper(&display_name, &config, &global_config).await?;
    }
    vm_success!("Restarted '{display_name}'");
    vm_hint!("Connect with: vm shell {display_name}");
    Ok(())
}

#[cfg(test)]
#[path = "tests/lifecycle.rs"]
mod tests;
