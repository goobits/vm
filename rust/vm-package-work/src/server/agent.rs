use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use vm_packages::{
    CheckoutLease, CleanupRequest, CreateCheckout, IntegrationRequest, LeaseRequest,
    SubmissionRecord, TransitionRequest, ValidationRequest, WorkflowState,
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
    Json(mut request): Json<CreateCheckout>,
) -> WorkResult<(StatusCode, Json<CheckoutLease>)> {
    state.store.source(&request.package).await?;
    if let Some(consumer) = &access.0 {
        request.agent = consumer.clone();
        request.consumers = vec![consumer.clone()];
        request.task = "managed guest package work".into();
        let matching = state
            .store
            .list_checkouts()
            .await?
            .into_iter()
            .filter(|checkout| {
                checkout.package == request.package
                    && checkout.workspace_release == request.workspace_release
                    && checkout.consumers.len() == 1
                    && checkout.consumers[0] == *consumer
                    && !checkout.state.revokes_lease()
            })
            .collect::<Vec<_>>();
        if matching.len() > 1 {
            return Err(WorkError::Conflict(format!(
                "{} active checkouts match this consumer and source; run `vm packages doctor --fix` on the controller host",
                matching.len()
            )));
        }
        if let Some(existing) = matching.into_iter().next() {
            let checkout = state
                .store
                .reacquire_lease(
                    &existing.checkout_id,
                    LeaseRequest {
                        holder: request.agent.clone(),
                        lease_token: request.lease_token.clone(),
                        duration_seconds: 8 * 60 * 60,
                        idempotency_key: format!("resume-{}", request.idempotency_key),
                    },
                )
                .await?;
            let mut lease = CheckoutLease {
                checkout,
                lease_token: Some(request.lease_token),
            };
            if !lease.checkout.workspace_release {
                lease.checkout = state.source.prepare(&state.store, &lease).await?;
            }
            return Ok((StatusCode::OK, Json(lease)));
        }
    } else {
        ensure_requested_consumer(&access, &request.consumers)?;
    }
    let mut checkout = state.store.create_checkout(request).await?;
    if !checkout.checkout.workspace_release {
        checkout.checkout = state.source.prepare(&state.store, &checkout).await?;
    }
    Ok((StatusCode::CREATED, Json(checkout)))
}

pub(super) async fn renew_lease(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(mut request): Json<LeaseRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    ensure_checkout_access(&state.store, &access, &id).await?;
    if access.0.is_some() {
        request.holder = state.store.get_checkout(&id).await?.agent;
        Ok(Json(state.store.reacquire_lease(&id, request).await?))
    } else {
        Ok(Json(state.store.renew_lease(&id, request).await?))
    }
}

pub(super) async fn release_lease(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(mut request): Json<LeaseRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    ensure_checkout_access(&state.store, &access, &id).await?;
    if access.0.is_some() {
        request.holder = state.store.get_checkout(&id).await?.agent;
    }
    Ok(Json(state.store.release_lease(&id, request).await?))
}

pub(super) async fn transition(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(mut request): Json<TransitionRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    ensure_checkout_access(&state.store, &access, &id).await?;
    if access.0.is_some() {
        request.actor = state.store.get_checkout(&id).await?.agent;
    }
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
    let next = request.next;
    let transitioned = state.store.transition(&id, request).await?;
    if access.0.is_some() && next == WorkflowState::Cancelled {
        return super::controller::cleanup_managed_checkout(
            &state,
            transitioned,
            CleanupRequest {
                actor: "package-agent".into(),
                idempotency_key: format!("agent-cleanup-{id}"),
            },
        )
        .await;
    }
    Ok(Json(transitioned))
}

pub(super) async fn validate_submission(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(mut request): Json<ValidationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    ensure_submission_access(&state.store, &access, &id).await?;
    if access.0.is_some() {
        request.actor = submission_actor(&state, &id).await?;
    }
    Ok(Json(state.store.validate_submission(&id, request).await?))
}

pub(super) async fn prepare_integration(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
    Json(mut request): Json<IntegrationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    ensure_submission_access(&state.store, &access, &id).await?;
    let submission = state.store.submission(&id).await?;
    if access.0.is_some() {
        request.actor = state
            .store
            .get_checkout(&submission.checkout_id)
            .await?
            .agent;
    }
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
    Json(mut request): Json<ValidationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    ensure_submission_access(&state.store, &access, &id).await?;
    if access.0.is_some() {
        request.actor = submission_actor(&state, &id).await?;
    }
    let completed = state.store.complete_integration(&id, request).await?;
    if completed.state == WorkflowState::ReadyToRelease {
        state.source.compact_integrated_checkout(&completed).await?;
    }
    Ok(Json(completed))
}

async fn submission_actor(state: &AppState, submission_id: &str) -> WorkResult<String> {
    let submission = state.store.submission(submission_id).await?;
    Ok(state
        .store
        .get_checkout(&submission.checkout_id)
        .await?
        .agent)
}
