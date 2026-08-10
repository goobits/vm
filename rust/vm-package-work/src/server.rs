use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use vm_packages::{
    authorization_token, BeginReleaseRequest, CheckoutLease, CompleteReleaseRequest,
    CreateCheckout, IntegrationRequest, LeaseRequest, PackageDefinition, PublicationRequest,
    RegisterPackage, ReleaseRecord, ReviewRequest, SubmissionRecord, TransitionRequest,
    ValidationRequest, WorkflowState,
};

use crate::{SourceManager, Store, WorkError, WorkResult};

const MAX_SUBMISSION_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Clone)]
struct Access {
    read_token: String,
    controller_token: String,
    reviewer_token: String,
    release_token: String,
}

#[derive(Clone)]
struct AppState {
    store: Arc<Store>,
    source: SourceManager,
    access: Access,
}

pub fn router(
    store: Arc<Store>,
    read_token: impl Into<String>,
    controller_token: impl Into<String>,
    reviewer_token: impl Into<String>,
    release_token: impl Into<String>,
) -> Router {
    let source = SourceManager::new(store.root());
    let state = AppState {
        store,
        source,
        access: Access {
            read_token: read_token.into(),
            controller_token: controller_token.into(),
            reviewer_token: reviewer_token.into(),
            release_token: release_token.into(),
        },
    };
    let reads = Router::new()
        .route("/v1/packages", get(list_packages))
        .route("/v1/packages/{*name}", get(get_package))
        .route("/v1/checkouts", get(list_checkouts))
        .route("/v1/checkouts/{checkout_id}", get(get_checkout))
        .route("/v1/receipts/{receipt_id}", get(get_receipt))
        .route("/v1/submissions", get(list_submissions))
        .route("/v1/submissions/{submission_id}", get(get_submission))
        .route("/v1/releases", get(list_releases))
        .route("/v1/releases/{release_id}", get(get_release))
        .route(
            "/v1/checkouts/{checkout_id}/submission",
            get(get_checkout_submission),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), read_auth));
    let writes = Router::new()
        .route("/v1/packages", post(register_package))
        .route("/v1/checkouts", post(create_checkout))
        .route("/v1/checkouts/{checkout_id}/lease/renew", post(renew_lease))
        .route(
            "/v1/checkouts/{checkout_id}/lease/release",
            post(release_lease),
        )
        .route("/v1/checkouts/{checkout_id}/transition", post(transition))
        .route(
            "/v1/submissions/{submission_id}/validate",
            post(validate_submission),
        )
        .route(
            "/v1/submissions/{submission_id}/integrate",
            post(prepare_integration),
        )
        .route(
            "/v1/submissions/{submission_id}/integration/complete",
            post(complete_integration),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            controller_auth,
        ));
    let reviews = Router::new()
        .route(
            "/v1/submissions/{submission_id}/review",
            post(record_review),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), reviewer_auth));
    let releases = Router::new()
        .route(
            "/v1/submissions/{submission_id}/release",
            post(begin_release),
        )
        .route(
            "/v1/releases/{release_id}/publications",
            post(record_publication),
        )
        .route("/v1/releases/{release_id}/complete", post(complete_release))
        .route(
            "/v1/submissions/{submission_id}/release-bundle",
            get(download_release_bundle),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), release_auth));

    Router::new()
        .route("/health", get(health))
        .route("/v1/checkouts/{checkout_id}/archive", get(download_archive))
        .route(
            "/v1/checkouts/{checkout_id}/submission",
            post(upload_submission),
        )
        .route(
            "/v1/submissions/{submission_id}/integration",
            get(download_integration),
        )
        .merge(reads)
        .merge(writes)
        .merge(reviews)
        .merge(releases)
        .with_state(state)
}

pub async fn run(
    host: String,
    port: u16,
    data: PathBuf,
    read_token: String,
    controller_token: String,
    reviewer_token: String,
    release_token: String,
) -> WorkResult<()> {
    if read_token.trim().is_empty()
        || controller_token.trim().is_empty()
        || reviewer_token.trim().is_empty()
        || release_token.trim().is_empty()
    {
        return Err(WorkError::Invalid(
            "read, controller, reviewer, and release tokens are required".into(),
        ));
    }
    if read_token == controller_token
        || read_token == reviewer_token
        || controller_token == reviewer_token
        || read_token == release_token
        || controller_token == release_token
        || reviewer_token == release_token
    {
        return Err(WorkError::Invalid(
            "read, controller, reviewer, and release tokens must be distinct".into(),
        ));
    }
    let listener = TcpListener::bind((host.as_str(), port)).await?;
    let store = Arc::new(Store::open(data).await?);
    tracing::info!(host, port, "package-work service listening");
    axum::serve(
        listener,
        router(
            store,
            read_token,
            controller_token,
            reviewer_token,
            release_token,
        ),
    )
    .await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn list_checkouts(
    State(state): State<AppState>,
) -> WorkResult<Json<Vec<vm_packages::CheckoutRecord>>> {
    Ok(Json(state.store.list_checkouts().await?))
}

async fn list_packages(State(state): State<AppState>) -> WorkResult<Json<Vec<PackageDefinition>>> {
    Ok(Json(state.store.packages().await))
}

async fn get_package(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> WorkResult<Json<PackageDefinition>> {
    Ok(Json(state.store.package(&name).await?))
}

async fn get_checkout(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    Ok(Json(state.store.get_checkout(&checkout_id).await?))
}

async fn get_receipt(
    State(state): State<AppState>,
    Path(receipt_id): Path<String>,
) -> WorkResult<Json<vm_packages::WorkflowReceipt>> {
    Ok(Json(state.store.get_receipt(&receipt_id).await?))
}

async fn list_submissions(
    State(state): State<AppState>,
) -> WorkResult<Json<Vec<SubmissionRecord>>> {
    Ok(Json(state.store.submissions().await))
}

async fn get_submission(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
) -> WorkResult<Json<SubmissionRecord>> {
    Ok(Json(state.store.submission(&submission_id).await?))
}

async fn get_checkout_submission(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
) -> WorkResult<Json<SubmissionRecord>> {
    Ok(Json(state.store.checkout_submission(&checkout_id).await?))
}

async fn list_releases(State(state): State<AppState>) -> WorkResult<Json<Vec<ReleaseRecord>>> {
    Ok(Json(state.store.releases().await))
}

async fn get_release(
    State(state): State<AppState>,
    Path(release_id): Path<String>,
) -> WorkResult<Json<ReleaseRecord>> {
    Ok(Json(state.store.release(&release_id).await?))
}

async fn create_checkout(
    State(state): State<AppState>,
    Json(request): Json<CreateCheckout>,
) -> WorkResult<(StatusCode, Json<CheckoutLease>)> {
    state.store.package(&request.package).await?;
    let mut checkout = state.store.create_checkout(request).await?;
    checkout.checkout = state.source.prepare(&state.store, &checkout).await?;
    Ok((StatusCode::CREATED, Json(checkout)))
}

async fn register_package(
    State(state): State<AppState>,
    Json(request): Json<RegisterPackage>,
) -> WorkResult<(StatusCode, Json<PackageDefinition>)> {
    Ok((
        StatusCode::CREATED,
        Json(state.store.register_package(request).await?),
    ))
}

async fn renew_lease(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    Json(request): Json<LeaseRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    Ok(Json(state.store.renew_lease(&checkout_id, request).await?))
}

async fn release_lease(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    Json(request): Json<LeaseRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    Ok(Json(
        state.store.release_lease(&checkout_id, request).await?,
    ))
}

async fn transition(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    Json(request): Json<TransitionRequest>,
) -> WorkResult<Json<vm_packages::CheckoutRecord>> {
    Ok(Json(state.store.transition(&checkout_id, request).await?))
}

async fn validate_submission(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
    Json(request): Json<ValidationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    Ok(Json(
        state
            .store
            .validate_submission(&submission_id, request)
            .await?,
    ))
}

async fn prepare_integration(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
    Json(request): Json<IntegrationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    let submission = state.store.submission(&submission_id).await?;
    Ok(Json(
        state
            .source
            .prepare_integration(&state.store, &submission, request)
            .await?,
    ))
}

async fn complete_integration(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
    Json(request): Json<ValidationRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    Ok(Json(
        state
            .store
            .complete_integration(&submission_id, request)
            .await?,
    ))
}

async fn record_review(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
    Json(request): Json<ReviewRequest>,
) -> WorkResult<Json<SubmissionRecord>> {
    Ok(Json(
        state.store.record_review(&submission_id, request).await?,
    ))
}

async fn begin_release(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
    Json(request): Json<BeginReleaseRequest>,
) -> WorkResult<Json<ReleaseRecord>> {
    Ok(Json(
        state.store.begin_release(&submission_id, request).await?,
    ))
}

async fn record_publication(
    State(state): State<AppState>,
    Path(release_id): Path<String>,
    Json(request): Json<PublicationRequest>,
) -> WorkResult<Json<ReleaseRecord>> {
    Ok(Json(
        state.store.record_publication(&release_id, request).await?,
    ))
}

async fn complete_release(
    State(state): State<AppState>,
    Path(release_id): Path<String>,
    Json(request): Json<CompleteReleaseRequest>,
) -> WorkResult<Json<ReleaseRecord>> {
    Ok(Json(
        state.store.complete_release(&release_id, request).await?,
    ))
}

#[derive(serde::Deserialize)]
struct ArchiveQuery {
    consumer: String,
}

async fn download_archive(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    Query(query): Query<ArchiveQuery>,
    headers: HeaderMap,
) -> WorkResult<Response> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(authorization_token)
        .ok_or_else(|| WorkError::Unauthorized("missing checkout lease credential".into()))?;
    let checkout = state
        .store
        .authorize_lease(&checkout_id, &query.consumer, &token)
        .await?;
    let archive = state.source.archive(&checkout).await?;
    let file = tokio::fs::File::open(archive).await?;
    let body = Body::from_stream(ReaderStream::new(file));
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-git-bundle"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=checkout.bundle",
            ),
        ],
        body,
    )
        .into_response())
}

async fn upload_submission(
    State(state): State<AppState>,
    Path(checkout_id): Path<String>,
    Query(query): Query<ArchiveQuery>,
    headers: HeaderMap,
    body: Body,
) -> WorkResult<(StatusCode, Json<SubmissionRecord>)> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(authorization_token)
        .ok_or_else(|| WorkError::Unauthorized("missing checkout lease credential".into()))?;
    let checkout = state
        .store
        .authorize_lease(&checkout_id, &query.consumer, &token)
        .await?;
    let staging = state.source.submission_staging_path(&checkout).await?;
    let result = async {
        let mut file = tokio::fs::File::create(&staging).await?;
        let mut stream = body.into_data_stream();
        let mut written = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| {
                WorkError::Invalid(format!("failed to read submitted bundle: {error}"))
            })?;
            written = written
                .checked_add(chunk.len() as u64)
                .ok_or_else(|| WorkError::Invalid("submitted bundle is too large".into()))?;
            if written > MAX_SUBMISSION_BYTES {
                return Err(WorkError::Invalid(format!(
                    "submitted bundle exceeds {MAX_SUBMISSION_BYTES} bytes"
                )));
            }
            file.write_all(&chunk).await?;
        }
        file.sync_all().await?;
        drop(file);
        if written == 0 {
            return Err(WorkError::Invalid("submitted bundle is empty".into()));
        }
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

async fn download_integration(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
    Query(query): Query<ArchiveQuery>,
    headers: HeaderMap,
) -> WorkResult<Response> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(authorization_token)
        .ok_or_else(|| WorkError::Unauthorized("missing checkout lease credential".into()))?;
    let submission = state.store.submission(&submission_id).await?;
    state
        .store
        .authorize_lease(&submission.checkout_id, &query.consumer, &token)
        .await?;
    let bundle = state.source.integration_bundle(&submission)?;
    let file = tokio::fs::File::open(bundle).await?;
    let body = Body::from_stream(ReaderStream::new(file));
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-git-bundle"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=integration.bundle",
            ),
        ],
        body,
    )
        .into_response())
}

async fn download_release_bundle(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
) -> WorkResult<Response> {
    let submission = state.store.submission(&submission_id).await?;
    if !matches!(
        submission.state,
        WorkflowState::ReadyToRelease | WorkflowState::Publishing | WorkflowState::Published
    ) {
        return Err(WorkError::Conflict(
            "submission is not ready for release".into(),
        ));
    }
    let bundle = state.source.integration_bundle(&submission)?;
    let file = tokio::fs::File::open(bundle).await?;
    let body = Body::from_stream(ReaderStream::new(file));
    Ok((
        [
            (header::CONTENT_TYPE, "application/x-git-bundle"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=release.bundle",
            ),
        ],
        body,
    )
        .into_response())
}

async fn read_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    let token = request_token(&request)?;
    if token != state.access.read_token
        && token != state.access.controller_token
        && token != state.access.reviewer_token
        && token != state.access.release_token
    {
        return Err(WorkError::Unauthorized("invalid read credential".into()));
    }
    Ok(next.run(request).await)
}

async fn release_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    let token = request_token(&request)?;
    if token != state.access.release_token && token != state.access.controller_token {
        return Err(WorkError::Unauthorized("invalid release credential".into()));
    }
    Ok(next.run(request).await)
}

async fn reviewer_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    let token = request_token(&request)?;
    if token != state.access.reviewer_token && token != state.access.controller_token {
        return Err(WorkError::Unauthorized(
            "invalid reviewer credential".into(),
        ));
    }
    Ok(next.run(request).await)
}

async fn controller_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    if request_token(&request)? != state.access.controller_token {
        return Err(WorkError::Unauthorized(
            "invalid controller credential".into(),
        ));
    }
    Ok(next.run(request).await)
}

fn request_token(request: &Request) -> WorkResult<String> {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(authorization_token)
        .ok_or_else(|| WorkError::Unauthorized("missing authorization credential".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    fn checkout() -> serde_json::Value {
        serde_json::json!({
            "package": "auth",
            "agent": "agent-1",
            "consumers": ["project-a"],
            "task": "refresh tokens",
            "idempotency_key": "create-1"
        })
    }

    #[tokio::test]
    async fn health_is_public_and_workflow_access_is_scoped() {
        let directory = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(directory.path()).await.unwrap());
        let server = TestServer::new(router(store, "read", "controller", "reviewer", "release"));

        assert_eq!(server.get("/health").await.status_code(), StatusCode::OK);
        assert_eq!(
            server.get("/v1/checkouts").await.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .get("/v1/checkouts")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .await
                .status_code(),
            StatusCode::OK
        );
        assert_eq!(
            server
                .post("/v1/checkouts")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .json(&checkout())
                .await
                .status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .get("/v1/checkouts")
                .add_header(header::AUTHORIZATION, "Bearer release")
                .await
                .status_code(),
            StatusCode::OK
        );
        assert_eq!(
            server
                .post("/v1/packages")
                .add_header(header::AUTHORIZATION, "Bearer release")
                .json(&serde_json::json!({
                    "name": "blocked",
                    "ecosystem": "cargo",
                    "repository": "https://example.com/blocked.git",
                    "default_branch": "main"
                }))
                .await
                .status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .post("/v1/packages")
                .add_header(header::AUTHORIZATION, "Bearer controller")
                .json(&serde_json::json!({
                    "name": "auth",
                    "ecosystem": "cargo",
                    "repository": "https://example.com/auth.git",
                    "default_branch": "main",
                    "ci_registry": null
                }))
                .await
                .status_code(),
            StatusCode::CREATED
        );
    }
}
