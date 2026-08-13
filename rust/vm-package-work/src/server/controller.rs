use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use vm_packages::{
    CleanupRequest, ConsumerRecord, CreateRollout, PackageDefinition, RegisterConsumer,
    RegisterPackage, RolloutRecord, WorkflowState,
};

use super::AppState;
use crate::{WorkError, WorkResult};

pub(super) async fn register_package(
    State(state): State<AppState>,
    Json(request): Json<RegisterPackage>,
) -> WorkResult<(StatusCode, Json<PackageDefinition>)> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.register_package(request).await?),
    ))
}

pub(super) async fn register_consumer(
    State(state): State<AppState>,
    Json(request): Json<RegisterConsumer>,
) -> WorkResult<(StatusCode, Json<ConsumerRecord>)> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.register_consumer(request).await?),
    ))
}

pub(super) async fn create_rollout(
    State(state): State<AppState>,
    Json(request): Json<CreateRollout>,
) -> WorkResult<(StatusCode, Json<RolloutRecord>)> {
    let rollout = state.store.create_rollout(request).await?;
    Ok((
        StatusCode::CREATED,
        Json(state.source.prepare_rollout(&state.store, &rollout).await?),
    ))
}

pub(super) async fn cleanup_checkout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CleanupRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    let checkout = state.store.get_checkout(&id).await?;
    if !matches!(
        checkout.state,
        WorkflowState::Published
            | WorkflowState::Rejected
            | WorkflowState::Cancelled
            | WorkflowState::Failed
            | WorkflowState::Closed
    ) {
        return Err(WorkError::Conflict(
            "only a terminal checkout can be cleaned up".into(),
        ));
    }
    cleanup_managed_checkout(&state, checkout, request).await
}

pub(super) async fn cleanup_managed_checkout(
    state: &AppState,
    checkout: vm_packages::CheckoutRecord,
    request: CleanupRequest,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    state.source.cleanup_checkout(&checkout).await?;
    Ok(Json(
        state
            .store
            .close_checkout(&checkout.checkout_id, request)
            .await?,
    ))
}
