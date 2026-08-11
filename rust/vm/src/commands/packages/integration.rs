use std::collections::BTreeMap;
use std::path::PathBuf;

use vm_core::{vm_progress, vm_success};
use vm_packages::{
    CheckOutcome, IntegrationRequest, PackageInfrastructureClient, RegistryEndpoints,
    ValidationRequest, WorkflowState,
};

use crate::commands::command_context::{load_runtime_subject, project_name};
use crate::error::{VmError, VmResult};

use super::{
    appliance::configured_state_and_client,
    files::ApplianceFiles,
    runtime::{checkout_root, exec, gateway_for_provider},
    submission::{run_consumer_check, run_package_check},
};

pub(super) async fn handle(
    files: &ApplianceFiles,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    submission_id: String,
    requested_consumer: Option<String>,
    strategy: String,
) -> VmResult<()> {
    let subject = load_runtime_subject(config_path, profile, None)?;
    let current_project = project_name(&subject.config).to_string();
    let consumer = requested_consumer.unwrap_or_else(|| current_project.clone());
    if consumer != current_project {
        return Err(VmError::validation(
            format!("Consumer '{consumer}' is not the current project '{current_project}'"),
            Some("Run this command from the selected consumer project"),
        ));
    }
    let (state, client) = configured_state_and_client(files)?;
    let submission = client.submission(&submission_id).await?;
    if submission.state != WorkflowState::Approved {
        return Err(VmError::validation(
            "Submission is not approved for integration",
            Some("Submit it and resolve any integration-review feedback first"),
        ));
    }
    let checkout = client.checkout(&submission.checkout_id).await?;
    if !checkout
        .consumers
        .iter()
        .any(|candidate| candidate == &consumer)
    {
        return Err(VmError::validation(
            "Submission is not assigned to this consumer",
            None::<String>,
        ));
    }
    let definition = client.package_definition(&submission.package).await?;
    vm_progress!("Integrating against the latest canonical package revision...");
    let integrating = client
        .prepare_integration(
            &submission.submission_id,
            &IntegrationRequest {
                actor: "vm-controller".into(),
                strategy,
                idempotency_key: format!("integrate-{}", submission.submission_id),
            },
        )
        .await?;
    let integration = integrating
        .integration
        .as_ref()
        .ok_or_else(|| VmError::validation("Integration record is missing", None::<String>))?;
    let checkout_root = checkout_root(&checkout.checkout_id)?;
    let root = format!("{checkout_root}/integration-{}", integrating.submission_id);
    let source = format!("{root}/source");
    let bundle = format!("{root}/integration.bundle");
    exec(&subject, ["mkdir", "-p", root.as_str()])?;
    let gateway = gateway_for_provider(&state, subject.provider.name())?;
    let download_client =
        PackageInfrastructureClient::new(RegistryEndpoints::new(gateway).map_err(VmError::from)?);
    let url = download_client.integration_bundle_url(&integrating.submission_id, &consumer);
    exec(
        &subject,
        [
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--header",
            &format!("@{checkout_root}/authorization-header"),
            &url,
            "--output",
            &bundle,
        ],
    )?;
    exec(&subject, ["git", "clone", &bundle, source.as_str()])?;
    exec(
        &subject,
        [
            "git",
            "-C",
            source.as_str(),
            "checkout",
            "--detach",
            &integration.integration_commit,
        ],
    )?;
    exec(&subject, ["rm", "-f", bundle.as_str()])?;
    vm_progress!("Rerunning integrated package and consumer checks...");
    run_package_check(&subject, definition.ecosystem, &source)?;
    run_consumer_check(&subject, definition.ecosystem, &submission.package, &source)?;
    let integration_commit = integration.integration_commit.clone();
    let ready = client
        .complete_integration(
            &integrating.submission_id,
            &ValidationRequest {
                package: CheckOutcome::Passed,
                consumers: BTreeMap::from([(consumer, CheckOutcome::Passed)]),
                actor: "vm-controller".into(),
                idempotency_key: format!("integration-checks-{}", integrating.submission_id),
            },
        )
        .await?;
    vm_success!(
        "Submission {} is ready to release at {}",
        ready.submission_id,
        integration_commit
    );
    Ok(())
}
