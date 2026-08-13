use std::path::Path as FilePath;

use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use vm_packages::{
    authorization_token, RolloutRecord, RolloutState, SubmissionRecord, WorkflowState,
};

use super::AppState;
use crate::{WorkError, WorkResult};

const MAX_SUBMISSION_BYTES: u64 = 256 * 1024 * 1024;

#[derive(serde::Deserialize)]
pub(super) struct ArchiveQuery {
    consumer: String,
}

fn lease_token(headers: &HeaderMap) -> WorkResult<String> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(authorization_token)
        .ok_or_else(|| WorkError::Unauthorized("missing checkout lease credential".into()))
}

pub(super) async fn download_archive(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ArchiveQuery>,
    headers: HeaderMap,
) -> WorkResult<Response> {
    let checkout = state
        .store
        .authorize_lease(&id, &query.consumer, &lease_token(&headers)?)
        .await?;
    download(state.source.archive(&checkout).await?, "checkout.bundle").await
}

pub(super) async fn upload_submission(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ArchiveQuery>,
    headers: HeaderMap,
    body: Body,
) -> WorkResult<(StatusCode, Json<SubmissionRecord>)> {
    let checkout = state
        .store
        .authorize_lease(&id, &query.consumer, &lease_token(&headers)?)
        .await?;
    let staging = state.source.submission_staging_path(&checkout).await?;
    let result = async {
        receive_bundle(body, &staging, "submitted").await?;
        state
            .source
            .import_submission(&state.store, &checkout, &staging)
            .await
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&staging).await;
    }
    Ok((StatusCode::CREATED, Json(result?)))
}

pub(super) async fn download_rollout(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> WorkResult<Response> {
    let rollout = state.store.rollout(&id).await?;
    if rollout.state != RolloutState::Active {
        return Err(WorkError::Conflict("rollout is not active".into()));
    }
    download(
        state.source.rollout_bundle(&rollout).await?,
        "rollout.bundle",
    )
    .await
}

pub(super) async fn upload_rollout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Body,
) -> WorkResult<(StatusCode, Json<RolloutRecord>)> {
    let rollout = state.store.rollout(&id).await?;
    if rollout.state != RolloutState::Active {
        return Err(WorkError::Conflict("rollout is not active".into()));
    }
    let staging = state.source.rollout_staging_path(&rollout).await?;
    let result = async {
        receive_bundle(body, &staging, "rollout").await?;
        state
            .source
            .import_rollout(&state.store, &rollout, &staging)
            .await
    }
    .await;
    let _ = tokio::fs::remove_file(&staging).await;
    Ok((StatusCode::CREATED, Json(result?)))
}

pub(super) async fn download_integration(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ArchiveQuery>,
    headers: HeaderMap,
) -> WorkResult<Response> {
    let submission = state.store.submission(&id).await?;
    state
        .store
        .authorize_lease(
            &submission.checkout_id,
            &query.consumer,
            &lease_token(&headers)?,
        )
        .await?;
    download(
        state.source.integration_bundle(&submission)?,
        "integration.bundle",
    )
    .await
}

pub(super) async fn download_release_bundle(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> WorkResult<Response> {
    let submission = state.store.submission(&id).await?;
    if !matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing | WorkflowState::Published
    ) {
        return Err(WorkError::Conflict(
            "submission is not ready for release".into(),
        ));
    }
    download(
        state.source.integration_bundle(&submission)?,
        "release.bundle",
    )
    .await
}

async fn download(path: std::path::PathBuf, filename: &'static str) -> WorkResult<Response> {
    let file = tokio::fs::File::open(path).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-git-bundle"),
            (
                header::CONTENT_DISPOSITION,
                match filename {
                    "checkout.bundle" => "attachment; filename=checkout.bundle",
                    "rollout.bundle" => "attachment; filename=rollout.bundle",
                    "integration.bundle" => "attachment; filename=integration.bundle",
                    _ => "attachment; filename=release.bundle",
                },
            ),
        ],
        Body::from_stream(ReaderStream::new(file)),
    )
        .into_response())
}

async fn receive_bundle(body: Body, path: &FilePath, label: &str) -> WorkResult<()> {
    let mut file = tokio::fs::File::create(path).await?;
    let mut stream = body.into_data_stream();
    let mut written = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            WorkError::Invalid(format!("failed to read {label} bundle: {error}"))
        })?;
        written = written
            .checked_add(chunk.len() as u64)
            .ok_or_else(|| WorkError::Invalid(format!("{label} bundle is too large")))?;
        if written > MAX_SUBMISSION_BYTES {
            return Err(WorkError::Invalid(format!(
                "{label} bundle exceeds {MAX_SUBMISSION_BYTES} bytes"
            )));
        }
        file.write_all(&chunk).await?;
    }
    file.sync_all().await?;
    if written == 0 {
        return Err(WorkError::Invalid(format!("{label} bundle is empty")));
    }
    Ok(())
}
