use vm_core::{vm_hint, vm_success, vm_warning};
use vm_packages::{
    LeaseRequest, PackageEcosystem, SourceKind, ToolActivationState, ToolActivationTargetState,
    WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    checkout, integration,
    runtime::{checkout_root, exec_output, GuestRuntime},
    submission, workspace,
};

const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const ACTIVATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) async fn handle_guest() -> VmResult<()> {
    let subject = GuestRuntime::discover()?;
    let mut workspace = None;
    let resolved_checkout_id = match subject.current_checkout_id()? {
        Some(checkout_id) => checkout_id,
        None => match workspace::prepare(&subject).await? {
            workspace::WorkspacePreparation::Published {
                release,
                source_kind,
            } => {
                vm_success!("Released {}@{}", release.package, release.version);
                if source_kind != SourceKind::Package {
                    wait_for_tool_activation(&subject.client()?, &release.release_id).await?;
                }
                return Ok(());
            }
            workspace::WorkspacePreparation::Pending(prepared) => {
                let checkout_id = prepared.checkout_id.clone();
                workspace = Some(prepared);
                checkout_id
            }
        },
    };
    let checkout_id = resolved_checkout_id.as_str();
    let client = subject.client()?;
    let checkout = client.checkout(checkout_id).await?;
    if !checkout.consumers.contains(&subject.consumer().to_string()) {
        return Err(VmError::validation(
            "Checkout is not assigned to this managed environment",
            None::<String>,
        ));
    }
    renew_release_lease(&subject, &client, &checkout).await?;
    let mut package_ecosystem = if matches!(
        checkout.state,
        WorkflowState::Active | WorkflowState::NeedsChanges | WorkflowState::Submitted
    ) || (checkout.state == WorkflowState::Created
        && checkout.workspace_release)
    {
        checkout_package_ecosystem(&client, &checkout).await?
    } else {
        None
    };
    let mut current = match checkout.state {
        WorkflowState::Created if checkout.workspace_release => {
            let workspace = workspace.as_ref().ok_or_else(|| {
                VmError::validation(
                    "Workspace checkout must be released from its canonical source directory",
                    Some("Run `vm packages release` from the canonical workspace"),
                )
            })?;
            submission::handle_workspace(
                &subject,
                &client,
                &checkout,
                &workspace.source,
                package_ecosystem,
            )
            .await?
        }
        WorkflowState::Active | WorkflowState::NeedsChanges => match workspace.as_ref() {
            Some(workspace) => {
                submission::handle_workspace(
                    &subject,
                    &client,
                    &checkout,
                    &workspace.source,
                    package_ecosystem,
                )
                .await?
            }
            None => {
                submission::handle_guest(&subject, &client, &checkout, package_ecosystem).await?
            }
        },
        WorkflowState::Submitted => match workspace.as_ref() {
            Some(workspace) => {
                submission::resume_workspace(
                    &subject,
                    &client,
                    &checkout,
                    &workspace.source,
                    package_ecosystem,
                )
                .await?
            }
            None => {
                submission::resume_guest(&subject, &client, &checkout, package_ecosystem).await?
            }
        },
        WorkflowState::Validating
        | WorkflowState::Reviewing
        | WorkflowState::Approved
        | WorkflowState::Integrating
        | WorkflowState::ReadyToRelease
        | WorkflowState::Publishing
        | WorkflowState::Published
        | WorkflowState::Closed => client.checkout_submission(checkout_id).await?,
        state => {
            return Err(VmError::validation(
                format!("Checkout cannot be released from {state:?}"),
                Some("Inspect it with `vm packages show <checkout-id>`"),
            ))
        }
    };
    if let Some(workspace) = workspace.as_mut() {
        workspace.record_commit(&subject, &current.submitted_commit)?;
    }
    current = wait_for_review(&client, current, checkout.workspace_release).await?;
    if matches!(
        current.state,
        WorkflowState::Approved | WorkflowState::Integrating
    ) {
        if package_ecosystem.is_none() {
            package_ecosystem = checkout_package_ecosystem(&client, &checkout).await?;
        }
        current =
            integration::handle_guest(&subject, &client, &checkout, &current, package_ecosystem)
                .await?;
    }
    let published = wait_for_publication(&client, current, checkout.workspace_release).await?;
    let release_id = published.release_id.as_deref().ok_or_else(|| {
        VmError::validation("Published submission has no release record", None::<String>)
    })?;
    let release = client.release(release_id).await?;
    let managed_checkout = workspace.is_none();
    if let Some(workspace) = workspace.as_mut() {
        workspace.record_commit(&subject, &release.source_commit)?;
    }
    vm_success!("Released {}@{}", release.package, release.version);
    if checkout.source_kind != SourceKind::Package {
        wait_for_tool_activation(&client, release_id).await?;
    }
    if managed_checkout {
        let checkout = client.checkout(checkout_id).await?;
        if let Err(error) = checkout::cleanup_guest(&subject, &checkout) {
            vm_hint!("Published successfully; local checkout cleanup was skipped: {error}");
        }
    }
    Ok(())
}

async fn checkout_package_ecosystem(
    client: &vm_packages::PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<Option<PackageEcosystem>> {
    if checkout.source_kind != SourceKind::Package {
        return Ok(None);
    }
    Ok(Some(
        client
            .package_definition(&checkout.package)
            .await?
            .ecosystem,
    ))
}

async fn wait_for_tool_activation(
    client: &vm_packages::PackageInfrastructureClient,
    release_id: &str,
) -> VmResult<()> {
    let deadline = tokio::time::Instant::now() + ACTIVATION_TIMEOUT;
    loop {
        let activation = client.tool_activation_for_release(release_id).await?;
        let pending = activation
            .targets
            .iter()
            .filter(|target| {
                target.initially_running && target.state == ToolActivationTargetState::Pending
            })
            .count();
        let planned = !activation.targets.is_empty()
            || matches!(
                activation.state,
                ToolActivationState::Waiting | ToolActivationState::Complete
            );
        if planned && pending == 0 {
            return activation_result(&activation, false);
        }
        if tokio::time::Instant::now() >= deadline {
            return activation_result(&activation, true);
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn activation_result(
    activation: &vm_packages::ToolActivationRecord,
    timed_out: bool,
) -> VmResult<()> {
    let running_total = activation
        .targets
        .iter()
        .filter(|target| target.initially_running)
        .count();
    let active = activation
        .targets
        .iter()
        .filter(|target| {
            target.initially_running && target.state == ToolActivationTargetState::Active
        })
        .count();
    let pending = activation
        .targets
        .iter()
        .filter(|target| {
            target.initially_running && target.state == ToolActivationTargetState::Pending
        })
        .count();
    let failed = activation
        .targets
        .iter()
        .filter(|target| {
            target.initially_running && target.state == ToolActivationTargetState::Failed
        })
        .count();
    let deferred = activation
        .targets
        .iter()
        .filter(|target| {
            !target.initially_running && target.state == ToolActivationTargetState::Deferred
        })
        .count();

    if pending == 0 && failed == 0 && !timed_out {
        vm_success!("Activated in {active} of {running_total} running environments");
    } else {
        vm_warning!("Activated in {active} of {running_total} running environments");
    }
    vm_success!(
        "{deferred} stopped environment{} will update when started",
        if deferred == 1 { "" } else { "s" }
    );
    vm_success!("No environments or volumes recreated");
    if failed > 0 || pending > 0 || timed_out {
        return Err(VmError::validation(
            format!(
                "Tool activation remains incomplete: {pending} pending, {failed} failed"
            ),
            Some("Rerun `vm packages release` to resume, or run `vm packages doctor --fix` on the controller"),
        ));
    }
    Ok(())
}

async fn renew_release_lease(
    subject: &GuestRuntime,
    client: &vm_packages::PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    if checkout.state.revokes_lease() {
        return Ok(());
    }
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let header = exec_output(subject, ["cat", &format!("{root}/authorization-header")])?;
    let token = header
        .trim()
        .strip_prefix("Authorization: Bearer ")
        .ok_or_else(|| {
            VmError::validation(
                "Managed checkout lease credential is invalid",
                None::<String>,
            )
        })?;
    client
        .renew_lease(
            &checkout.checkout_id,
            &LeaseRequest {
                holder: checkout.agent.clone(),
                lease_token: token.into(),
                duration_seconds: 24 * 60 * 60,
                idempotency_key: release_lease_key(checkout),
            },
        )
        .await?;
    Ok(())
}

fn release_lease_key(checkout: &vm_packages::CheckoutRecord) -> String {
    let generation = checkout.lease.as_ref().map_or_else(
        || checkout.updated_at.timestamp_millis(),
        |lease| lease.expires_at.timestamp_millis(),
    );
    format!("release-lease-{}-{generation}", checkout.checkout_id)
}

async fn wait_for_review(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
    workspace_release: bool,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    while matches!(
        submission.state,
        WorkflowState::Submitted | WorkflowState::Validating | WorkflowState::Reviewing
    ) {
        if tokio::time::Instant::now() >= deadline {
            return Err(VmError::validation(
                "Timed out waiting for package review",
                Some("Rerun the same release command to resume"),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        submission = client.submission(&submission.submission_id).await?;
    }
    match submission.state {
        WorkflowState::NeedsChanges => Err(VmError::validation(
            submission
                .review
                .as_ref()
                .map(|review| format!("Package review requested changes: {}", review.reason))
                .unwrap_or_else(|| "Package review requested changes".into()),
            Some(if workspace_release {
                "Edit and commit the canonical workspace, then rerun `vm packages release`"
            } else {
                "Edit and commit the checkout, then rerun the same release command"
            }),
        )),
        WorkflowState::Rejected | WorkflowState::Failed => Err(VmError::validation(
            "Package review rejected or failed the release",
            Some("Inspect the checkout and review receipt"),
        )),
        _ => Ok(submission),
    }
}

async fn wait_for_publication(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
    workspace_release: bool,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    while matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing
    ) {
        if tokio::time::Instant::now() >= deadline {
            return Err(VmError::validation(
                "Timed out waiting for private package publication",
                Some("Rerun the same release command to resume"),
            ));
        }
        tokio::time::sleep(POLL_INTERVAL).await;
        submission = client.submission(&submission.submission_id).await?;
    }
    match submission.state {
        WorkflowState::Published | WorkflowState::Closed => Ok(submission),
        WorkflowState::NeedsChanges => Err(VmError::validation(
            submission
                .review
                .as_ref()
                .map(|review| format!("Package release requested changes: {}", review.reason))
                .unwrap_or_else(|| "Package release requested changes".into()),
            Some(if workspace_release {
                "Edit and commit the canonical workspace, then rerun `vm packages release`"
            } else {
                "Edit and commit the checkout, then rerun the same release command"
            }),
        )),
        state => Err(VmError::validation(
            format!("Package release stopped in {state:?}"),
            Some("Inspect package infrastructure logs and rerun when repaired"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn activation(
        states: &[(bool, ToolActivationTargetState)],
    ) -> vm_packages::ToolActivationRecord {
        let now = Utc::now();
        vm_packages::ToolActivationRecord {
            activation_id: "activate-rel-1".into(),
            release_id: "rel-1".into(),
            checkout_id: "checkout-1".into(),
            tool: "typemill".into(),
            version: "1.2.0".into(),
            source_commit: "a".repeat(40),
            state: ToolActivationState::Waiting,
            targets: states
                .iter()
                .enumerate()
                .map(
                    |(index, (running, state))| vm_packages::ToolActivationTarget {
                        target_id: format!("target-{index}"),
                        environment: format!("project-{index}"),
                        provider: "docker".into(),
                        initially_running: *running,
                        state: *state,
                        attempts: 1,
                        error: (*state == ToolActivationTargetState::Failed)
                            .then(|| "activation failed".into()),
                        updated_at: now,
                    },
                )
                .collect(),
            lease: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn activation_result_accepts_active_and_deferred_targets() {
        assert!(activation_result(
            &activation(&[
                (true, ToolActivationTargetState::Active),
                (false, ToolActivationTargetState::Deferred),
            ]),
            false,
        )
        .is_ok());
    }

    #[test]
    fn activation_result_rejects_failed_or_timed_out_targets() {
        assert!(activation_result(
            &activation(&[(true, ToolActivationTargetState::Failed)]),
            false,
        )
        .is_err());
        assert!(activation_result(
            &activation(&[(true, ToolActivationTargetState::Pending)]),
            true,
        )
        .is_err());
    }
}
