use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    middleware,
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::{cargo, npm, pypi, state::AppState};

const GUEST_CLIENT_PATH: &str = "/usr/local/lib/vm-packages/vm";

pub(super) fn app_router(state: AppState) -> Router {
    let is_worker_edge = state.internal_client.is_some();
    let config = state.config.clone();
    let reads = Router::new()
        .route("/", get(index_handler))
        .route("/status", get(status_handler))
        .route("/api/status", get(status_handler))
        .route("/setup.sh", get(setup_script_handler))
        .route("/vm-client", get(guest_client_handler))
        .route("/vm-client.sha256", get(guest_client_digest_handler))
        .route("/api/packages", get(list_packages_handler))
        .route("/npm/{package}/-/{filename}", get(npm::download_tarball))
        .route(
            "/npm/{scope}/{package}/-/{filename}",
            get(npm::download_scoped_tarball),
        )
        .route("/npm/{package}", get(npm::package_metadata))
        .route("/pypi/simple/", get(pypi::simple_index))
        .route("/pypi/simple/{package}/", get(pypi::package_index))
        .route("/pypi/packages/{filename}", get(pypi::download_file))
        .route("/pypi/internal/{*path}", get(pypi::download_internal_file))
        .route("/pypi/upstream/{*path}", get(pypi::download_upstream_file))
        .route("/cargo/index/{*path}", get(cargo::sparse_index))
        .route(
            "/cargo/api/v1/crates/{crate}/{version}/download",
            get(cargo::download_crate),
        )
        .route(
            "/cargo/api/v1/crates/{crate}",
            get(cargo::get_crate_versions_api),
        )
        .route(
            "/cargo/api/v1/crates/{crate}/{version}",
            get(cargo::download_crate),
        )
        .route_layer(middleware::from_fn_with_state(
            config,
            crate::auth::read_auth_middleware,
        ));
    let writes = Router::new()
        .route("/npm/{package}", put(npm::publish_package))
        .route(
            "/pypi/upload",
            post(pypi::upload_package).put(pypi::upload_package),
        )
        .route("/cargo/api/v1/crates/new", put(cargo::publish_crate));
    let mut app = Router::new()
        .route("/health", get(health_handler))
        .merge(reads);
    if !is_worker_edge {
        app = app.merge(writes).merge(crate::tools::router());
    }
    app.with_state(Arc::new(state))
        .layer(middleware::from_fn_with_state(
            vm_logging::HttpLogContext::new("package_registry"),
            vm_logging::request_context,
        ))
}

async fn guest_client_handler() -> Response {
    match tokio::fs::read(GUEST_CLIENT_PATH).await {
        Ok(client) => (
            [
                (header::CONTENT_TYPE, "application/octet-stream"),
                (header::CONTENT_DISPOSITION, "attachment; filename=vm"),
            ],
            client,
        )
            .into_response(),
        Err(error) => guest_client_error(error),
    }
}

async fn guest_client_digest_handler() -> Response {
    match tokio::fs::read(GUEST_CLIENT_PATH).await {
        Ok(client) => format!("{}\n", vm_packages::sha256_hex(&client)).into_response(),
        Err(error) => guest_client_error(error),
    }
}

fn guest_client_error(error: std::io::Error) -> Response {
    if error.kind() == std::io::ErrorKind::NotFound {
        return (StatusCode::NOT_FOUND, "guest VM client is unavailable").into_response();
    }
    error!(%error, "failed to read guest VM client");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "guest VM client is unavailable",
    )
        .into_response()
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

async fn status_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let data_dir = state.data_dir.to_string_lossy();
    info!(%data_dir, "reporting package server status");
    let response = serde_json::json!({
        "status": "ok",
        "service": "goobits-pkg-server",
        "version": env!("CARGO_PKG_VERSION"),
        "data_directory": data_dir,
        "registries": ["npm", "pypi", "cargo"],
    })
    .to_string();
    (StatusCode::OK, json_headers(), response)
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, json_headers(), r#"{"status": "healthy"}"#)
}

async fn list_packages_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let data_dir = state.data_dir.clone();
    match tokio::task::spawn_blocking(move || crate::local_storage::list_local_packages(&data_dir))
        .await
    {
        Ok(Ok(packages)) => (
            StatusCode::OK,
            json_headers(),
            serde_json::to_string(&packages).unwrap_or_else(|_| "{}".into()),
        ),
        Ok(Err(error)) => {
            error!(%error, "failed to list packages");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json_headers(),
                serde_json::json!({ "error": error.to_string() }).to_string(),
            )
        }
        Err(error) => {
            error!(%error, "package listing task failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                json_headers(),
                r#"{"error":"Internal Server Error"}"#.into(),
            )
        }
    }
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .expect("static header value is valid"),
    );
    headers
}

#[derive(Deserialize)]
struct SetupQuery {
    registry: Option<String>,
    port: Option<u16>,
}

async fn setup_script_handler(Query(params): Query<SetupQuery>) -> Response {
    let registry = params.registry.as_deref().unwrap_or("npm");
    let port = params.port.unwrap_or(8080);
    (
        [
            (header::CONTENT_TYPE, "text/plain"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"setup.sh\"",
            ),
        ],
        super::setup::client_script(registry, port),
    )
        .into_response()
}
