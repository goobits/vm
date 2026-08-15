use axum::{
    extract::{Path, State},
    Json,
};
use vm_packages::{
    BeginReleaseRequest, CleanupRequest, CompleteReleaseRequest, PublicationRequest, ReleaseRecord,
    ReleaseReworkRequest, ReviewRequest, RolloutRecord, RolloutState, RolloutValidationRequest,
    SubmissionRecord, WorkflowState,
};

use super::{controller::cleanup_managed_checkout, AppState};
use crate::{WorkError, WorkResult};

pub(super) async fn next_review(State(state): State<AppState>) -> Json<Option<SubmissionRecord>> {
    Json(state.store.next_review().await)
}

pub(super) async fn next_release(State(state): State<AppState>) -> Json<Option<SubmissionRecord>> {
    Json(state.store.next_release().await)
}

pub(super) async fn reconcile_rollout_queue(
    State(state): State<AppState>,
) -> Json<Option<RolloutRecord>> {
    prepare_rollout_queue(&state).await;
    Json(state.store.next_rollout().await)
}

async fn prepare_rollout_queue(state: &AppState) {
    let mut rollouts = state
        .store
        .rollouts()
        .await
        .into_iter()
        .filter(|rollout| rollout.state == RolloutState::Created)
        .collect::<Vec<_>>();
    match state.store.ensure_automatic_rollouts().await {
        Ok(created) => rollouts.extend(created),
        Err(error) => tracing::warn!(%error, "failed to reconcile automatic package rollouts"),
    }
    rollouts.sort_by_key(|rollout| rollout.created_at);
    let mut seen = std::collections::HashSet::new();
    rollouts.retain(|rollout| seen.insert(rollout.rollout_id.clone()));
    for rollout in rollouts {
        if let Err(error) = state.source.prepare_rollout(&state.store, &rollout).await {
            tracing::warn!(rollout_id = %rollout.rollout_id, %error, "failed to prepare package rollout");
        }
    }
}

pub(super) async fn record_review(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReviewRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    Ok(Json(state.store.record_review(&id, request).await?))
}

pub(super) async fn begin_release(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<BeginReleaseRequest>,
) -> WorkResult<Json<ReleaseRecord>> {
    Ok(Json(state.store.begin_release(&id, request).await?))
}

pub(super) async fn request_release_rework(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<ReleaseReworkRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    let submission = state.store.submission(&id).await?;
    let checkout = state.store.get_checkout(&submission.checkout_id).await?;
    state
        .source
        .restore_checkout(&state.store, &checkout)
        .await?;
    Ok(Json(
        state.store.request_release_rework(&id, request).await?,
    ))
}

pub(super) async fn record_publication(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<PublicationRequest>,
) -> WorkResult<Json<ReleaseRecord>> {
    Ok(Json(state.store.record_publication(&id, request).await?))
}

pub(super) async fn complete_release(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CompleteReleaseRequest>,
) -> WorkResult<Json<ReleaseRecord>> {
    Ok(Json(state.store.complete_release(&id, request).await?))
}

pub(super) async fn cleanup_release(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CleanupRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    let release = state.store.release(&id).await?;
    if release.state != WorkflowState::Published {
        return Err(WorkError::Conflict(
            "only a published release checkout can be cleaned up".into(),
        ));
    }
    let checkout = state.store.get_checkout(&release.checkout_id).await?;
    if !matches!(
        checkout.state,
        WorkflowState::Published | WorkflowState::Closed
    ) {
        return Err(WorkError::Conflict(
            "release checkout is not ready for cleanup".into(),
        ));
    }
    cleanup_managed_checkout(&state, checkout, request).await
}

pub(super) async fn complete_rollout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<RolloutValidationRequest>,
) -> WorkResult<Json<RolloutRecord>> {
    let rollout = state.store.rollout(&id).await?;
    if request.passed {
        state.source.push_rollout(&state.store, &rollout).await?;
        state.source.cleanup_rollout(&rollout).await?;
    }
    let completed = state.store.complete_rollout(&id, request).await?;
    if completed.state != RolloutState::ReadyForReview {
        state.source.cleanup_rollout(&completed).await?;
    }
    Ok(Json(completed))
}
