use chrono::Utc;

use super::progress::{build_phase_label, release_phase_label, PollBackoff};
use super::wait::{activation_result, release_wait_timeout};
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

fn activation(states: &[(bool, ToolActivationTargetState)]) -> vm_packages::ToolActivationRecord {
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
        "receipt-release-1",
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
        "receipt-release-1",
    )
    .is_err());
    let timeout = activation_result(
        &activation(&[(true, ToolActivationTargetState::Pending)]),
        true,
        "receipt-release-1",
    )
    .unwrap_err();
    assert!(timeout.to_string().contains("activate-rel-1"));
    assert!(timeout.to_string().contains("deadline"));
    assert_eq!(timeout.exit_code(), 5);
    assert!(timeout
        .hint()
        .unwrap()
        .contains("vm packages release --receipt receipt-release-1"));
    let failed_after_start = activation(&[
        (true, ToolActivationTargetState::Active),
        (false, ToolActivationTargetState::Failed),
    ]);
    let partial = activation_result(&failed_after_start, false, "receipt-release-1").unwrap_err();
    assert_eq!(partial.exit_code(), 1);
}

#[test]
fn release_timeout_identifies_the_durable_receipt_and_resume_command() {
    let now = Utc::now();
    let submission = vm_packages::SubmissionRecord {
        submission_id: "submission-1".into(),
        checkout_id: "checkout-1".into(),
        package: "shared".into(),
        branch: "release".into(),
        base_commit: "a".repeat(40),
        submitted_commit: "b".repeat(40),
        diff_digest: "c".repeat(64),
        state: WorkflowState::Publishing,
        validation: None,
        review: None,
        integration: None,
        build_progress: None,
        release_id: Some("release-1".into()),
        created_at: now,
        updated_at: now,
    };
    let error = release_wait_timeout(&submission, "receipt-release-1");
    assert!(error.to_string().contains("receipt-release-1"));
    assert!(error.to_string().contains("deadline"));
    assert!(error
        .hint()
        .unwrap()
        .contains("vm packages release --receipt receipt-release-1"));
    assert_eq!(error.exit_code(), 5);
}

#[test]
fn release_interruption_identifies_receipt_and_uses_interrupt_status() {
    let error = release_interruption(Some("receipt-release-1"));
    assert_eq!(error.exit_code(), 130);
    assert!(error.to_string().contains("receipt-release-1"));
    assert!(error
        .hint()
        .unwrap()
        .contains("vm packages release --receipt receipt-release-1"));

    let before_receipt = release_interruption(None);
    assert_eq!(before_receipt.exit_code(), 130);
    assert!(before_receipt.to_string().contains("before a receipt"));
}
