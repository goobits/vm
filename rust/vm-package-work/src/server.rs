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
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use vm_packages::{
    authorization_token, CheckoutLease, CreateCheckout, LeaseRequest, PackageDefinition,
    RegisterPackage, TransitionRequest,
};

use crate::{SourceManager, Store, WorkError, WorkResult};

#[derive(Clone)]
struct Access {
    read_token: String,
    controller_token: String,
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
) -> Router {
    let source = SourceManager::new(store.root());
    let state = AppState {
        store,
        source,
        access: Access {
            read_token: read_token.into(),
            controller_token: controller_token.into(),
        },
    };
    let reads = Router::new()
        .route("/v1/packages", get(list_packages))
        .route("/v1/packages/{*name}", get(get_package))
        .route("/v1/checkouts", get(list_checkouts))
        .route("/v1/checkouts/{checkout_id}", get(get_checkout))
        .route("/v1/receipts/{receipt_id}", get(get_receipt))
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
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            controller_auth,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/v1/checkouts/{checkout_id}/archive", get(download_archive))
        .merge(reads)
        .merge(writes)
        .with_state(state)
}

pub async fn run(
    host: String,
    port: u16,
    data: PathBuf,
    read_token: String,
    controller_token: String,
) -> WorkResult<()> {
    if read_token.trim().is_empty() || controller_token.trim().is_empty() {
        return Err(WorkError::Invalid(
            "read and controller tokens are required".into(),
        ));
    }
    if read_token == controller_token {
        return Err(WorkError::Invalid(
            "read and controller tokens must be distinct".into(),
        ));
    }
    let listener = TcpListener::bind((host.as_str(), port)).await?;
    let store = Arc::new(Store::open(data).await?);
    tracing::info!(host, port, "package-work service listening");
    axum::serve(listener, router(store, read_token, controller_token)).await?;
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

async fn read_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> WorkResult<Response> {
    let token = request_token(&request)?;
    if token != state.access.read_token && token != state.access.controller_token {
        return Err(WorkError::Unauthorized("invalid read credential".into()));
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
        let server = TestServer::new(router(store, "read", "controller"));

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
                .post("/v1/packages")
                .add_header(header::AUTHORIZATION, "Bearer controller")
                .json(&serde_json::json!({
                    "name": "auth",
                    "ecosystem": "cargo",
                    "repository": "https://example.com/auth.git",
                    "default_branch": "main"
                }))
                .await
                .status_code(),
            StatusCode::CREATED
        );
    }
}
