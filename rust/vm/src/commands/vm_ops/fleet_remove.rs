//! Project-scoped bulk environment removal.

use crate::cli::FleetArgs;
use crate::error::VmResult;
use vm_config::GlobalConfig;

use super::destroy::{confirm_removal, handle_destroy};
use super::fleet::{
    configured_provider_for_instance, project_targets, selected_config_for_instance, FleetProgress,
    FleetProject,
};
use super::targets::InstanceStateFilter;

pub async fn handle_fleet_remove(
    targets: &FleetArgs,
    project: &FleetProject,
    delete_data: bool,
    yes: bool,
) -> VmResult<()> {
    let instances = project_targets(targets, InstanceStateFilter::Any, project)?;
    let names = instances
        .iter()
        .map(|instance| format!("{} ({})", instance.name, instance.provider))
        .collect::<Vec<_>>();
    if !confirm_removal(&names, delete_data, yes)? {
        return Ok(());
    }
    let global_config = GlobalConfig::load()?;
    let mut progress = FleetProgress::default();
    for instance in instances {
        let config = selected_config_for_instance(project, &instance);
        let outcome = match configured_provider_for_instance(project, &instance) {
            Ok(provider) => {
                handle_destroy(
                    provider,
                    Some(&instance.name),
                    config,
                    global_config.clone(),
                    delete_data,
                )
                .await
            }
            Err(error) => Err(error),
        };
        match outcome {
            Ok(()) => progress.success(&instance.name),
            Err(error) => progress.failure(&instance.name, &error),
        }
    }
    progress.finish()
}
