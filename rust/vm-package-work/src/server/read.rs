use axum::{
    extract::{Extension, Path, State},
    Json,
};
use vm_packages::{
    ConsumerRecord, ConsumerUsage, InternalPackageCatalog, PackageDefinition, PackageDrift,
    ReleaseRecord, RolloutRecord, SubmissionRecord,
};

use super::{
    auth::{checkout_is_visible, ensure_checkout_is_visible, visible_checkout_ids, AgentAccess},
    AppState,
};
use crate::{WorkError, WorkResult};

pub(super) async fn list_checkouts(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
) -> WorkResult<Json<Vec<vm_packages::CheckoutRecord>>> {
    let mut checkouts = state.store.list_checkouts().await?;
    checkouts.retain(|checkout| checkout_is_visible(&access, checkout));
    Ok(Json(checkouts))
}

pub(super) async fn list_packages(
    State(state): State<AppState>,
) -> WorkResult<Json<Vec<PackageDefinition>>> {
    Ok(Json(state.store.packages().await))
}

pub(super) async fn get_catalog(
    State(state): State<AppState>,
) -> WorkResult<Json<InternalPackageCatalog>> {
    Ok(Json(state.store.internal_catalog().await?))
}

pub(super) async fn get_package(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> WorkResult<Json<PackageDefinition>> {
    Ok(Json(state.store.package(&name).await?))
}

pub(super) async fn get_checkout(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    let checkout = state.store.get_checkout(&id).await?;
    ensure_checkout_is_visible(&access, &checkout)?;
    Ok(Json(checkout))
}

pub(super) async fn get_receipt(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
) -> WorkResult<Json<vm_packages::WorkflowReceipt>> {
    let receipt = state.store.get_receipt(&id).await?;
    ensure_checkout_is_visible(
        &access,
        &state.store.get_checkout(&receipt.checkout_id).await?,
    )?;
    Ok(Json(receipt))
}

pub(super) async fn list_submissions(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
) -> WorkResult<Json<Vec<SubmissionRecord>>> {
    let visible = visible_checkout_ids(&state.store, &access).await?;
    let mut records = state.store.submissions().await;
    records.retain(|record| visible.contains(&record.checkout_id));
    Ok(Json(records))
}

pub(super) async fn get_submission(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
) -> WorkResult<Json<SubmissionRecord>> {
    let record = state.store.submission(&id).await?;
    ensure_checkout_is_visible(
        &access,
        &state.store.get_checkout(&record.checkout_id).await?,
    )?;
    Ok(Json(record))
}

pub(super) async fn get_tool_build(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
) -> WorkResult<Json<vm_packages::ToolBuildRecord>> {
    let submission = state.store.submission(&id).await?;
    ensure_checkout_is_visible(
        &access,
        &state.store.get_checkout(&submission.checkout_id).await?,
    )?;
    Ok(Json(state.store.tool_build(&id).await?))
}

pub(super) async fn get_checkout_submission(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
) -> WorkResult<Json<SubmissionRecord>> {
    ensure_checkout_is_visible(&access, &state.store.get_checkout(&id).await?)?;
    Ok(Json(state.store.checkout_submission(&id).await?))
}

pub(super) async fn list_releases(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
) -> WorkResult<Json<Vec<ReleaseRecord>>> {
    let visible = visible_checkout_ids(&state.store, &access).await?;
    let mut records = state.store.releases().await;
    records.retain(|record| visible.contains(&record.checkout_id));
    Ok(Json(records))
}

pub(super) async fn get_release(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
) -> WorkResult<Json<ReleaseRecord>> {
    let record = state.store.release(&id).await?;
    ensure_checkout_is_visible(
        &access,
        &state.store.get_checkout(&record.checkout_id).await?,
    )?;
    Ok(Json(record))
}

pub(super) async fn list_consumers(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
) -> WorkResult<Json<Vec<ConsumerRecord>>> {
    let mut records = state.store.consumers().await;
    if let Some(expected) = &access.0 {
        records.retain(|record| &record.name == expected);
    }
    Ok(Json(records))
}

pub(super) async fn package_consumers(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(name): Path<String>,
) -> WorkResult<Json<Vec<ConsumerUsage>>> {
    let mut records = state.store.package_consumers(&name).await?;
    if let Some(expected) = &access.0 {
        records.retain(|record| &record.consumer == expected);
    }
    Ok(Json(records))
}

pub(super) async fn drift(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
) -> WorkResult<Json<Vec<PackageDrift>>> {
    let mut records = state.store.drift().await;
    if let Some(expected) = &access.0 {
        for record in &mut records {
            record
                .consumers
                .retain(|consumer| &consumer.consumer == expected);
        }
        records.retain(|record| !record.consumers.is_empty());
    }
    Ok(Json(records))
}

pub(super) async fn list_rollouts(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
) -> WorkResult<Json<Vec<RolloutRecord>>> {
    let mut records = state.store.rollouts().await;
    if let Some(expected) = &access.0 {
        records.retain(|record| &record.consumer == expected);
    }
    Ok(Json(records))
}

pub(super) async fn get_rollout(
    State(state): State<AppState>,
    Extension(access): Extension<AgentAccess>,
    Path(id): Path<String>,
) -> WorkResult<Json<RolloutRecord>> {
    let record = state.store.rollout(&id).await?;
    if access
        .0
        .as_ref()
        .is_some_and(|expected| expected != &record.consumer)
    {
        return Err(WorkError::Unauthorized(
            "package agent credential is bound to a different consumer".into(),
        ));
    }
    Ok(Json(record))
}
