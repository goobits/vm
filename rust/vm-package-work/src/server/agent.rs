use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use vm_packages::{
    CheckoutLease, CreateCheckout, IntegrationRequest, LeaseRequest, SubmissionRecord,
    TransitionRequest, ValidationRequest, WorkflowState,
};

use super::{
    auth::{
        ensure_checkout_access, ensure_requested_consumer, ensure_submission_access, AgentAccess,
    },
    AppState,
};
use crate::{WorkError, WorkResult};

pub(super) async fn create_checkout(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Json(request): Json<CreateCheckout>,
) -> WorkResult<(StatusCode, Json<CheckoutLease>)> {
    ensure_requested_consumer(&access, &request.consumers)?;
    state.store.source(&request.package).await?;
    let mut checkout = state.store.create_checkout(request).await?;
    checkout.checkout = state.source.prepare(&state.store, &checkout).await?;
    Ok((StatusCode::CREATED, Json(checkout)))
}

pub(super) async fn renew_lease(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(request): Json<LeaseRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    ensure_checkout_access(&state.store, &access, &id).await?;
    Ok(Json(state.store.renew_lease(&id, request).await?))
}

pub(super) async fn release_lease(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(request): Json<LeaseRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    ensure_checkout_access(&state.store, &access, &id).await?;
    Ok(Json(state.store.release_lease(&id, request).await?))
}

pub(super) async fn transition(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(request): Json<TransitionRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    ensure_checkout_access(&state.store, &access, &id).await?;
    if access.0.is_some()
        && !matches!(
            request.next,
            WorkflowState::Active | WorkflowState::Cancelled | WorkflowState::Failed
        )
    {
        return Err(WorkError::Unauthorized(
            "package agents cannot perform this workflow transition".into(),
        ));
    }
    Ok(Json(state.store.transition(&id, request).await?))
}

pub(super) async fn validate_submission(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(request): Json<ValidationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    ensure_submission_access(&state.store, &access, &id).await?;
    Ok(Json(state.store.validate_submission(&id, request).await?))
}

pub(super) async fn prepare_integration(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(request): Json<IntegrationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    ensure_submission_access(&state.store, &access, &id).await?;
    let submission = state.store.submission(&id).await?;
    Ok(Json(
        state
            .source
            .prepare_integration(&state.store, &submission, request)
            .await?,
    ))
}

pub(super) async fn complete_integration(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(request): Json<ValidationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    ensure_submission_access(&state.store, &access, &id).await?;
    let completed = state.store.complete_integration(&id, request).await?;
    if completed.state == WorkflowState::ReadyToRelease {
        state.source.compact_integrated_checkout(&completed).await?;
    }
    Ok(Json(completed))
}
