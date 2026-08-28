use vm_core::{vm_hint, vm_println, vm_success, vm_warning};
use vm_packages::{
    LeaseRequest, PackageEcosystem, SourceKind, ToolActivationState, ToolActivationTargetState,
    ToolBuildPhase, WorkflowState,
};

use crate::error::{VmError, VmResult};

use super::{
    checkout,
    guest_checkout::{checkout_root, read_file},
    guest_runtime::GuestRuntime,
    integration, submission, workspace,
};

const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
const ACTIVATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);
const INITIAL_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
const MAX_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

struct ReleaseProgress {
    last_state: WorkflowState,
    last_build_progress: Option<(ToolBuildPhase, Option<String>)>,
    next_heartbeat: tokio::time::Instant,
}

impl ReleaseProgress {
    fn new(submission: &vm_packages::SubmissionRecord) -> Self {
        Self {
            last_state: submission.state,
            last_build_progress: build_progress_key(submission),
            next_heartbeat: tokio::time::Instant::now() + PROGRESS_INTERVAL,
        }
    }

    fn report(&mut self, submission: &vm_packages::SubmissionRecord) {
        let now = tokio::time::Instant::now();
        let build_progress = build_progress_key(submission);
        if submission.state != self.last_state
            || build_progress != self.last_build_progress
            || now >= self.next_heartbeat
        {
            print_release_phase(submission);
            self.last_state = submission.state;
            self.last_build_progress = build_progress;
            self.next_heartbeat = now + PROGRESS_INTERVAL;
        }
    }
}

fn build_progress_key(
    submission: &vm_packages::SubmissionRecord,
) -> Option<(ToolBuildPhase, Option<String>)> {
    submission
        .build_progress
        .as_ref()
        .map(|progress| (progress.phase, progress.target.clone()))
}

struct PollBackoff {
    next: std::time::Duration,
}

impl PollBackoff {
    fn new() -> Self {
        Self {
            next: INITIAL_POLL_INTERVAL,
        }
    }

    fn next_delay(&mut self) -> std::time::Duration {
        let delay = self.next;
        self.next = self.next.saturating_mul(2).min(MAX_POLL_INTERVAL);
        delay
    }

    async fn wait(&mut self, deadline: tokio::time::Instant) -> bool {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        tokio::time::sleep(self.next_delay().min(remaining)).await;
        tokio::time::Instant::now() < deadline
    }
}

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
    if !checkout
        .consumers
        .iter()
        .any(|consumer| consumer == subject.consumer())
    {
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
        workspace.record_commit(&current.submitted_commit)?;
    }
    if !matches!(
        current.state,
        WorkflowState::Published | WorkflowState::Closed
    ) {
        vm_println!("Release job: {}", current.submission_id);
        print_release_phase(&current);
        vm_hint!("Ctrl-C detaches without cancelling; rerun `vm packages release` to resume");
        if workspace.is_none() {
            vm_hint!("Cancel explicitly with `vm packages cancel` from this checkout");
        }
    }
    current = wait_for_review(&client, current, checkout.workspace_release).await?;
    if matches!(
        current.state,
        WorkflowState::Approved | WorkflowState::Integrating
    ) {
        vm_println!("Phase: integrating approved source");
        if package_ecosystem.is_none() {
            package_ecosystem = checkout_package_ecosystem(&client, &checkout).await?;
        }
        current =
            integration::handle_guest(&subject, &client, &checkout, &current, package_ecosystem)
                .await?;
        print_release_phase(&current);
    }
    let published = wait_for_publication(&client, current, checkout.workspace_release).await?;
    let release_id = published.release_id.as_deref().ok_or_else(|| {
        VmError::validation("Published submission has no release record", None::<String>)
    })?;
    let release = client.release(release_id).await?;
    let managed_checkout = workspace.is_none();
    if let Some(workspace) = workspace.as_mut() {
        workspace.record_commit(&release.source_commit)?;
    }
    vm_success!("Released {}@{}", release.package, release.version);
    if checkout.source_kind != SourceKind::Package {
        wait_for_tool_activation(&client, release_id).await?;
    }
    if managed_checkout {
        if let Err(error) =
            checkout::cleanup_guest_after_release(&subject, &checkout, &release.source_commit)
        {
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
    let mut poll = PollBackoff::new();
    let mut last_counts = None;
    let mut next_heartbeat = tokio::time::Instant::now();
    loop {
        let activation = client.tool_activation_for_release(release_id).await?;
        let counts = activation_counts(&activation);
        let now = tokio::time::Instant::now();
        if last_counts.as_ref() != Some(&counts) || now >= next_heartbeat {
            vm_println!(
                "Phase: activating privately ({}/{} running; {} pending; {} failed)",
                counts.active,
                counts.running_total,
                counts.pending,
                counts.failed
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
            return activation_result(&activation, false);
        }
        if !poll.wait(deadline).await {
            return activation_result(&activation, true);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ActivationCounts {
    running_total: usize,
    active: usize,
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
        pending: activation
            .targets
            .iter()
            .filter(|target| {
                target.initially_running && target.state == ToolActivationTargetState::Pending
            })
            .count(),
        failed: activation
            .targets
            .iter()
            .filter(|target| {
                target.initially_running && target.state == ToolActivationTargetState::Failed
            })
            .count(),
        deferred: activation
            .targets
            .iter()
            .filter(|target| {
                !target.initially_running && target.state == ToolActivationTargetState::Deferred
            })
            .count(),
    }
}

fn activation_result(
    activation: &vm_packages::ToolActivationRecord,
    timed_out: bool,
) -> VmResult<()> {
    let counts = activation_counts(activation);

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
    vm_success!(
        "{} stopped environment{} will update when started",
        counts.deferred,
        if counts.deferred == 1 { "" } else { "s" }
    );
    vm_success!("No environments or volumes recreated");
    if counts.failed > 0 || counts.pending > 0 || timed_out {
        return Err(VmError::validation(
            format!(
                "Tool activation remains incomplete: {} pending, {} failed",
                counts.pending, counts.failed
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

async fn wait_for_review(
    client: &vm_packages::PackageInfrastructureClient,
    mut submission: vm_packages::SubmissionRecord,
    workspace_release: bool,
) -> VmResult<vm_packages::SubmissionRecord> {
    let deadline = tokio::time::Instant::now() + RELEASE_TIMEOUT;
    let mut poll = PollBackoff::new();
    let mut progress = ReleaseProgress::new(&submission);
    while matches!(
        submission.state,
        WorkflowState::Submitted | WorkflowState::Validating | WorkflowState::Reviewing
    ) {
        if !poll.wait(deadline).await {
            return Err(VmError::validation(
                format!(
                    "Timed out waiting for release job {} in {}",
                    submission.submission_id,
                    release_phase_label(submission.state)
                ),
                Some(format!(
                    "Inspect with `vm packages show {}`, then rerun `vm packages release` to resume",
                    submission.checkout_id
                )),
            ));
        }
        submission = client.submission(&submission.submission_id).await?;
        progress.report(&submission);
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
    let mut poll = PollBackoff::new();
    let mut progress = ReleaseProgress::new(&submission);
    while matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing
    ) {
        if !poll.wait(deadline).await {
            return Err(VmError::validation(
                format!(
                    "Timed out waiting for release job {} in {}",
                    submission.submission_id,
                    release_phase_label(submission.state)
                ),
                Some(format!(
                    "Inspect with `vm packages show {}`, then rerun `vm packages release` to resume",
                    submission.checkout_id
                )),
            ));
        }
        submission = client.submission(&submission.submission_id).await?;
        progress.report(&submission);
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

fn print_release_phase(submission: &vm_packages::SubmissionRecord) {
    let phase = if submission.state == WorkflowState::ReadyToRelease {
        submission
            .build_progress
            .as_ref()
            .map(build_phase_label)
            .unwrap_or_else(|| release_phase_label(submission.state).into())
    } else {
        release_phase_label(submission.state).into()
    };
    vm_println!("Phase: {} (job {})", phase, submission.submission_id);
}

fn build_phase_label(progress: &vm_packages::ToolBuildProgress) -> String {
    match progress.phase {
        ToolBuildPhase::Preparing => "preparing isolated source".into(),
        ToolBuildPhase::RestoringDependencies => "restoring locked dependencies".into(),
        ToolBuildPhase::Building => progress.target.as_ref().map_or_else(
            || "building binary tool".into(),
            |target| format!("building binary tool for {target}"),
        ),
        ToolBuildPhase::Staging => "verifying and staging artifacts".into(),
        ToolBuildPhase::Complete => "isolated build complete".into(),
        ToolBuildPhase::Failed => "isolated build failed".into(),
    }
}

fn release_phase_label(state: WorkflowState) -> &'static str {
    match state {
        WorkflowState::Created | WorkflowState::CheckedOut | WorkflowState::Active => "preparing",
        WorkflowState::Submitted => "submitted",
        WorkflowState::Validating => "validating",
        WorkflowState::Reviewing => "reviewing",
        WorkflowState::Approved => "approved",
        WorkflowState::Integrating => "integrating",
        WorkflowState::ReadyToRelease => "queued for isolated build/publication",
        WorkflowState::Publishing => "publishing privately",
        WorkflowState::Published => "published",
        WorkflowState::NeedsChanges => "needs changes",
        WorkflowState::Rejected => "rejected",
        WorkflowState::Cancelled => "cancelled",
        WorkflowState::Failed => "failed",
        WorkflowState::Closed => "closed",
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn release_phase_labels_explain_queue_and_terminal_states() {
        assert_eq!(
            release_phase_label(WorkflowState::ReadyToRelease),
            "queued for isolated build/publication"
        );
        assert_eq!(
            release_phase_label(WorkflowState::Publishing),
            "publishing privately"
        );
        assert_eq!(release_phase_label(WorkflowState::Failed), "failed");
    }

    #[test]
    fn binary_build_progress_names_the_active_target() {
        let progress = vm_packages::ToolBuildProgress {
            attempt: "vm-build-test".into(),
            phase: ToolBuildPhase::Building,
            target: Some("linux-arm64".into()),
            actor: "tool-build-service".into(),
            started_at: Utc::now(),
            updated_at: Utc::now(),
        };

        assert_eq!(
            build_phase_label(&progress),
            "building binary tool for linux-arm64"
        );
    }

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
    fn polling_starts_fast_and_stays_bounded() {
        let mut poll = PollBackoff::new();

        assert_eq!(poll.next_delay(), std::time::Duration::from_millis(250));
        assert_eq!(poll.next_delay(), std::time::Duration::from_millis(500));
        assert_eq!(poll.next_delay(), std::time::Duration::from_secs(1));
        assert_eq!(poll.next_delay(), MAX_POLL_INTERVAL);
        assert_eq!(poll.next_delay(), MAX_POLL_INTERVAL);
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
