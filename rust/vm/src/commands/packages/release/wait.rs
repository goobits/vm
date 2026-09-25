use super::progress::{release_phase_label, PollBackoff, ReleaseProgress};
use super::*;
use std::future::Future;

async fn observe_durable<T>(
    future: impl Future<Output = VmResult<T>>,
    deadline: tokio::time::Instant,
    timeout_error: impl FnOnce() -> VmError,
) -> VmResult<T> {
    tokio::time::timeout_at(deadline, future)
        .await
        .unwrap_or_else(|_| Err(timeout_error()))
}

pub(super) async fn submission_receipt(
    client: &vm_packages::PackageInfrastructureClient,
    checkout_id: &str,
) -> VmResult<String> {
    client
        .checkout(checkout_id)
        .await?
        .transitions
        .iter()
        .rev()
        .find(|transition| transition.next == WorkflowState::Submitted)
        .map(|transition| transition.receipt_id.clone())
        .ok_or_else(|| {
            VmError::validation(
                "The release has no persisted submission receipt yet",
                Some("Rerun `vm packages release` to inspect its state"),
            )
        })
}

pub(super) async fn checkout_package_ecosystem(
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

pub(super) async fn wait_for_tool_activation(
    client: &vm_packages::PackageInfrastructureClient,
    release_id: &str,
    receipt_id: &str,
) -> VmResult<()> {
    let deadline = tokio::time::Instant::now() + ACTIVATION_TIMEOUT;
    let mut poll = PollBackoff::new();
    let mut last_counts = None;
    let mut next_heartbeat = tokio::time::Instant::now();
    loop {
        let activation = observe_durable(
            async { Ok(client.tool_activation_for_release(release_id).await?) },
            deadline,
            || activation_wait_timeout(release_id, receipt_id),
        )
        .await?;
        let counts = activation_counts(&activation);
        let now = tokio::time::Instant::now();
        if last_counts.as_ref() != Some(&counts) || now >= next_heartbeat {
            vm_println!(
                "Phase: activating privately ({}/{} initially running; {} activated after start; {} pending; {} failed; {} deferred)",
                counts.active,
                counts.running_total,
                counts.active_after_start,
                counts.pending,
                counts.failed,
                counts.deferred,
            );
            last_counts = Some(counts);
            next_heartbeat = now + PROGRESS_INTERVAL;
        }
        let planned = !activation.targets.is_empty()
            || matches!(
                activation.state,
                ToolActivationState::Waiting | ToolActivationState::Complete
            );
        if planned && counts.pending == 0 {
            return activation_result(&activation, false, receipt_id);
        }
        if !observe_durable(async { Ok(poll.wait(deadline).await) }, deadline, || {
            activation_wait_timeout(release_id, receipt_id)
        })
        .await?
        {
            return activation_result(&activation, true, receipt_id);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivationCounts {
    running_total: usize,
    active: usize,
    active_after_start: usize,
    pending: usize,
    failed: usize,
    deferred: usize,
}

fn activation_counts(activation: &vm_packages::ToolActivationRecord) -> ActivationCounts {
    ActivationCounts {
        running_total: activation
            .targets
            .iter()
            .filter(|target| target.initially_running)
            .count(),
        active: activation
            .targets
            .iter()
            .filter(|target| {
                target.initially_running && target.state == ToolActivationTargetState::Active
            })
            .count(),
        active_after_start: activation
            .targets
            .iter()
            .filter(|target| {
                !target.initially_running && target.state == ToolActivationTargetState::Active
            })
            .count(),
        pending: activation
            .targets
            .iter()
            .filter(|target| target.state == ToolActivationTargetState::Pending)
            .count(),
        failed: activation
            .targets
            .iter()
            .filter(|target| target.state == ToolActivationTargetState::Failed)
            .count(),
        deferred: activation
            .targets
            .iter()
            .filter(|target| target.state == ToolActivationTargetState::Deferred)
            .count(),
    }
}

pub(super) fn activation_result(
    activation: &vm_packages::ToolActivationRecord,
    timed_out: bool,
    receipt_id: &str,
) -> VmResult<()> {
    let counts = activation_counts(activation);
    vm_println!("Activation: {}", activation.activation_id);

    if counts.pending == 0 && counts.failed == 0 && !timed_out {
        vm_success!(
            "Activated in {} of {} running environments",
            counts.active,
            counts.running_total
        );
    } else {
        vm_warning!(
            "Activated in {} of {} running environments",
            counts.active,
            counts.running_total
        );
    }
    vm_println!(
        "{} stopped environment{} still deferred; {} activated after start",
        counts.deferred,
        if counts.deferred == 1 { "" } else { "s" },
        counts.active_after_start,
    );
    vm_println!("No environments or volumes recreated");
    if counts.failed > 0 || counts.pending > 0 || timed_out {
        let message = format!(
            "Tool activation '{}' {}: {} pending, {} failed; durable work may continue",
            activation.activation_id,
            if timed_out {
                "exceeded its wait deadline"
            } else {
                "remains incomplete"
            },
            counts.pending,
            counts.failed
        );
        let hint = Some(format!(
            "Inspect or resume with `vm packages release --receipt {receipt_id}`; inspect the selected environment with `vm tools status`"
        ));
        return Err(if timed_out {
            VmError::deadline(message, hint)
        } else {
            VmError::operation(message, hint)
        });
    }
    Ok(())
}

pub(super) async fn renew_release_lease(
    subject: &GuestRuntime,
    client: &vm_packages::PackageInfrastructureClient,
    checkout: &vm_packages::CheckoutRecord,
) -> VmResult<()> {
    if checkout.state.revokes_lease() {
        return Ok(());
    }
    let root = checkout_root(subject, &checkout.checkout_id)?;
    let header = read_file(&format!("{root}/authorization-header"))?;
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

pub(super) async fn wait_for_review(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
    workspace_release: bool,
    receipt_id: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    let mut poll = PollBackoff::new();
    let mut progress = ReleaseProgress::new(&submission);
    while matches!(
        submission.state,
        WorkflowState::Submitted | WorkflowState::Validating | WorkflowState::Reviewing
    ) {
        if !observe_durable(async { Ok(poll.wait(deadline).await) }, deadline, || {
            release_wait_timeout(&submission, receipt_id)
        })
        .await?
        {
            return Err(release_wait_timeout(&submission, receipt_id));
        }
        submission = observe_durable(
            async { Ok(client.submission(&submission.submission_id).await?) },
            deadline,
            || release_wait_timeout(&submission, receipt_id),
        )
        .await?;
        progress.report(&submission);
    }
    match submission.state {
        WorkflowState::NeedsChanges => Err(VmError::conflict(
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
        WorkflowState::Rejected | WorkflowState::Failed => Err(VmError::operation(
            "Package review rejected or failed the release",
            Some("Inspect the checkout and review receipt"),
        )),
        _ => Ok(submission),
    }
}

pub(super) async fn wait_for_publication(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
    workspace_release: bool,
    receipt_id: &str,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    let mut poll = PollBackoff::new();
    let mut progress = ReleaseProgress::new(&submission);
    while matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing
    ) {
        if !observe_durable(async { Ok(poll.wait(deadline).await) }, deadline, || {
            release_wait_timeout(&submission, receipt_id)
        })
        .await?
        {
            return Err(release_wait_timeout(&submission, receipt_id));
        }
        submission = observe_durable(
            async { Ok(client.submission(&submission.submission_id).await?) },
            deadline,
            || release_wait_timeout(&submission, receipt_id),
        )
        .await?;
        progress.report(&submission);
    }
    match submission.state {
        WorkflowState::Published | WorkflowState::Closed => Ok(submission),
        WorkflowState::NeedsChanges => Err(VmError::conflict(
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
        state => Err(VmError::operation(
            format!("Package release stopped in {state:?}"),
            Some("Inspect package infrastructure logs and rerun when repaired"),
        )),
    }
}

pub(super) fn release_wait_timeout(
    submission: &vm_packages::SubmissionRecord,
    receipt_id: &str,
) -> VmError {
    VmError::deadline(
        format!(
            "Release job {} exceeded its wait deadline in {}; receipt {receipt_id} remains durable",
            submission.submission_id,
            release_phase_label(submission.state)
        ),
        Some(format!(
            "Work may continue; inspect or resume with `vm packages release --receipt {receipt_id}`"
        )),
    )
}

fn activation_wait_timeout(release_id: &str, receipt_id: &str) -> VmError {
    VmError::deadline(
        format!("Tool activation for release {release_id} exceeded its wait deadline; receipt {receipt_id} remains durable"),
        Some(format!("Work may continue; inspect or resume with `vm packages release --receipt {receipt_id}`")),
    )
}
