use super::*;

pub(super) struct ReleaseProgress {
    last_state: WorkflowState,
    last_build_progress: Option<(ToolBuildPhase, Option<String>)>,
    next_heartbeat: tokio::time::Instant,
}

impl ReleaseProgress {
    pub(super) fn new(submission: &vm_packages::SubmissionRecord) -> Self {
        Self {
            last_state: submission.state,
            last_build_progress: build_progress_key(submission),
            next_heartbeat: tokio::time::Instant::now() + PROGRESS_INTERVAL,
        }
    }

    pub(super) fn report(&mut self, submission: &vm_packages::SubmissionRecord) {
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

pub(super) struct PollBackoff {
    next: std::time::Duration,
}

impl PollBackoff {
    pub(super) fn new() -> Self {
        Self {
            next: INITIAL_POLL_INTERVAL,
        }
    }

    pub(super) fn next_delay(&mut self) -> std::time::Duration {
        let delay = self.next;
        self.next = self.next.saturating_mul(2).min(MAX_POLL_INTERVAL);
        delay
    }

    pub(super) async fn wait(&mut self, deadline: tokio::time::Instant) -> bool {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        tokio::time::sleep(self.next_delay().min(remaining)).await;
        tokio::time::Instant::now() < deadline
    }
}

pub(super) fn print_release_phase(submission: &vm_packages::SubmissionRecord) {
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

pub(super) fn build_phase_label(progress: &vm_packages::ToolBuildProgress) -> String {
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

pub(super) fn release_phase_label(state: WorkflowState) -> &'static str {
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
