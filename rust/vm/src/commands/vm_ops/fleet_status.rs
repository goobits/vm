//! Structured fleet status collection.

use crate::cli::FleetArgs;
use crate::commands::status::{self, StatusOutput, StatusTarget};
use crate::error::VmResult;
use crate::presentation::ErrorRecord;

use super::fleet::{configured_provider_for_instance, project_targets, FleetProject};
use super::targets::InstanceStateFilter;

pub(in crate::commands) fn collect(
    targets: &FleetArgs,
    project: &FleetProject,
) -> VmResult<(StatusOutput, Vec<ErrorRecord>)> {
    let mut instances = project_targets(targets, InstanceStateFilter::Any, project)?;
    instances.sort_by(|a, b| a.provider.cmp(&b.provider).then(a.name.cmp(&b.name)));

    let mut results = Vec::with_capacity(instances.len());
    let mut errors = Vec::new();
    for instance in instances {
        let result = (|| {
            let provider = configured_provider_for_instance(project, &instance)?;
            status::collect(provider.as_ref(), &instance.name)
        })();
        match result {
            Ok(status) => results.push(StatusTarget::success(
                instance.name,
                instance.provider,
                status,
            )),
            Err(error) => {
                results.push(StatusTarget::failure(
                    instance.name,
                    instance.provider,
                    &error,
                ));
                errors.push(ErrorRecord::from_error(&error));
            }
        }
    }
    Ok((StatusOutput::from_targets(results), errors))
}
