//! HTTP server setup and route handlers for package registry operations
//!
//! This module contains the complete Axum-based HTTP server implementation
//! supporting npm, PyPI, and Cargo package registry operations.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    middleware,
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{debug, error, info, warn};

use crate::{
    cargo,
    config::Config,
    npm, pypi,
    resolver::{ResolverService, CATALOG_REFRESH_INTERVAL},
    state::AppState,
    upstream::{UpstreamClient, UpstreamConfig},
    InternalRegistryClient,
};
use vm_core::validation as core_validation;

pub async fn run_server_background(host: String, port: u16, data_dir: PathBuf) -> Result<()> {
    run_server_with_shutdown(host, port, data_dir, None).await
}

pub async fn run_server_with_shutdown(
    host: String,
    port: u16,
    data_dir: PathBuf,
    shutdown_receiver: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    run_server_internal(host, port, data_dir, shutdown_receiver).await
}

pub async fn run_server(host: String, port: u16, data_dir: PathBuf) -> Result<()> {
    run_server_internal(host, port, data_dir, None).await
}

async fn run_server_internal(
    host: String,
    port: u16,
    data_dir: PathBuf,
    shutdown_receiver: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<()> {
    info!("🚀 Starting Goobits Package Server...");

    if let Err(e) = core_validation::validate_hostname(&host) {
        error!(host = %host, error = %e, "❌ Invalid host parameter: {}", e);
        anyhow::bail!("Invalid host parameter: {}", e);
    }

    if let Err(e) = crate::validation::validate_docker_port(port) {
        error!(port = %port, error = %e, "❌ Invalid port parameter: {}", e);
        anyhow::bail!("Invalid port parameter: {}", e);
    }

    let abs_data_dir = match std::fs::canonicalize(&data_dir) {
        Ok(path) => path,
        Err(_) => {
            std::fs::create_dir_all(&data_dir)?;
            match std::env::current_dir() {
                Ok(current) => current.join(&data_dir),
                Err(e) => {
                    error!(error = %e, "❌ Failed to get current directory: {}", e);
                    anyhow::bail!("Failed to get current directory: {}", e);
                }
            }
        }
    };

    info!("📂 Using data directory: {}", abs_data_dir.display());
    info!("Starting server on {host}:{port}");

    // Create required components for AppState
    let upstream_config = UpstreamConfig::default();
    let upstream_client = Arc::new(UpstreamClient::new(upstream_config)?);
    let internal_client = InternalRegistryClient::from_environment()?.map(Arc::new);
    let read_token = std::env::var("PKG_SERVER_READ_TOKEN").ok();
    let publish_token = std::env::var("PKG_SERVER_PUBLISH_TOKEN")
        .ok()
        .or_else(|| std::env::var("PKG_SERVER_AUTH_TOKEN").ok());
    let config = Arc::new(configure_security(
        &host,
        read_token.as_deref(),
        publish_token.as_deref(),
        internal_client.is_some(),
    )?);
    let server_addr = format!("http://{host}:{port}");
    let resolver = Arc::new(ResolverService::from_environment(
        &abs_data_dir,
        internal_client.clone(),
    ));
    if internal_client.is_some() {
        start_catalog_refresh(Arc::clone(&resolver));
    }

    let state = AppState {
        data_dir: abs_data_dir,
        server_addr,
        upstream_client,
        internal_client,
        config,
        resolver,
    };

    let app = app_router(state);

    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        error!(host = %host, port = %port, error = %e, "Invalid socket address");
        anyhow::anyhow!("Invalid socket address {host}:{port}: {e}")
    })?;

    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        error!(addr = %addr, error = %e, "Failed to bind to address");
        anyhow::anyhow!("Failed to bind to {host}:{port}: {e}")
    })?;

    info!("✅ Server is running on http://{}:{}", host, port);
    info!("Server listening on {}", addr);

    match shutdown_receiver {
        Some(shutdown_rx) => {
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    info!("Received shutdown signal, stopping server gracefully");
                })
                .await
                .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;
        }
        None => {
            axum::serve(listener, app)
                .await
                .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;
        }
    }

    Ok(())
}

fn app_router(state: AppState) -> Router {
    let is_worker_edge = state.internal_client.is_some();
    let config = state.config.clone();
    let reads = Router::new()
        .route("/", get(index_handler))
        .route("/status", get(status_handler))
        .route("/api/status", get(status_handler))
        .route("/setup.sh", get(setup_script_handler))
        .route("/api/packages", get(list_packages_handler))
        .route("/npm/{package}/-/{filename}", get(npm::download_tarball))
        .route("/npm/{package}", get(npm::package_metadata))
        .route("/pypi/simple/", get(pypi::simple_index))
        .route("/pypi/simple/{package}/", get(pypi::package_index))
        .route("/pypi/packages/{filename}", get(pypi::download_file))
        .route("/pypi/internal/{*path}", get(pypi::download_internal_file))
        .route("/pypi/upstream/{*path}", get(pypi::download_upstream_file))
        .route("/pypi/legacy/api/pypi", get(pypi::simple_index))
        .route("/pypi/legacy/api/pypi/{package}/", get(pypi::package_index))
        .route(
            "/pypi/legacy/api/pypi/{package}/{version}",
            get(pypi::package_index),
        )
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
            config.clone(),
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
}

fn start_catalog_refresh(resolver: Arc<ResolverService>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CATALOG_REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut unavailable = false;
        loop {
            interval.tick().await;
            match resolver.refresh().await {
                Ok(()) if unavailable => {
                    info!("package catalog connection recovered");
                    unavailable = false;
                }
                Ok(()) => {}
                Err(error) if unavailable => {
                    debug!(error = %error, "package catalog remains unavailable");
                }
                Err(error) => {
                    warn!(error = %error, "package catalog is unavailable; using the last known snapshot");
                    unavailable = true;
                }
            }
        }
    });
}

fn configure_security(
    host: &str,
    read_token: Option<&str>,
    publish_token: Option<&str>,
    read_only: bool,
) -> Result<Config> {
    let mut config = Config::default();
    let read_token = read_token.filter(|token| !token.trim().is_empty());
    let publish_token = publish_token.filter(|token| !token.trim().is_empty());

    if !is_loopback_host(host) {
        if read_token.is_none() {
            anyhow::bail!(
                "Refusing to bind package server to non-loopback host '{host}' without a read token"
            );
        }
        if !read_only && publish_token.is_none() {
            anyhow::bail!(
                "Refusing to bind writable package server to non-loopback host '{host}' without a separate publish token"
            );
        }
    }

    if read_token.is_some() || publish_token.is_some() {
        config.security.require_authentication = true;
        config.security.read_keys = read_token.map(str::to_string).into_iter().collect();
        config.security.publish_keys = publish_token.map(str::to_string).into_iter().collect();
    }

    Ok(config)
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Json;
    use axum_test::TestServer;
    use vm_packages::{InternalPackageCatalog, PackageEcosystem, PackageIdentity};

    async fn spawn_http(app: Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), task)
    }

    fn edge_state(data_dir: &std::path::Path, gateway: &str) -> AppState {
        let internal = Arc::new(InternalRegistryClient::new(gateway, "read").unwrap());
        AppState {
            data_dir: data_dir.to_path_buf(),
            server_addr: "http://127.0.0.1:3080".into(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            internal_client: Some(Arc::clone(&internal)),
            config: Arc::new(configure_security("0.0.0.0", Some("read"), None, true).unwrap()),
            resolver: Arc::new(ResolverService::worker_edge(data_dir, internal)),
        }
    }

    #[test]
    fn loopback_bind_does_not_require_authentication() {
        let config = configure_security("127.0.0.1", None, None, false).unwrap();
        assert!(!config.security.require_authentication);
    }

    #[test]
    fn remote_bind_requires_and_enables_authentication() {
        assert!(configure_security("0.0.0.0", None, None, false).is_err());
        assert!(configure_security("0.0.0.0", Some("read"), None, false).is_err());

        let config = configure_security("0.0.0.0", Some("read"), Some("publish"), false).unwrap();
        assert!(config.security.require_authentication);
        assert_eq!(config.security.read_keys, ["read"]);
        assert_eq!(config.security.publish_keys, ["publish"]);
    }

    #[tokio::test]
    async fn router_separates_health_read_and_publish_access() {
        let directory = tempfile::tempdir().unwrap();
        let config =
            Arc::new(configure_security("0.0.0.0", Some("read"), Some("publish"), false).unwrap());
        let state = AppState {
            data_dir: directory.path().to_path_buf(),
            server_addr: "http://127.0.0.1:3080".into(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            internal_client: None,
            config,
            resolver: Arc::new(ResolverService::standalone()),
        };
        let server = TestServer::new(app_router(state));

        assert_eq!(server.get("/health").await.status_code(), StatusCode::OK);
        assert_eq!(
            server.get("/api/status").await.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .get("/api/status")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .await
                .status_code(),
            StatusCode::OK
        );
        assert_eq!(
            server
                .put("/npm/example")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .json(&serde_json::json!({"name": "example"}))
                .await
                .status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            server
                .put("/npm/example")
                .add_header(header::AUTHORIZATION, "Bearer publish")
                .json(&serde_json::json!({"name": "example"}))
                .await
                .status_code(),
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn worker_edge_exposes_no_publish_routes() {
        let directory = tempfile::tempdir().unwrap();
        let config = Arc::new(configure_security("0.0.0.0", Some("read"), None, true).unwrap());
        let state = AppState {
            data_dir: directory.path().to_path_buf(),
            server_addr: "http://127.0.0.1:3080".into(),
            upstream_client: Arc::new(UpstreamClient::disabled()),
            internal_client: Some(Arc::new(
                InternalRegistryClient::new("http://127.0.0.1:9", "read").unwrap(),
            )),
            config,
            resolver: Arc::new(ResolverService::standalone()),
        };
        let server = TestServer::new(app_router(state));

        assert_eq!(
            server
                .put("/npm/example")
                .add_header(header::AUTHORIZATION, "Bearer read")
                .json(&serde_json::json!({"name": "example"}))
                .await
                .status_code(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            server
                .put("/tools/artifacts/tool/1.0.0/any/deadbeef.tar.gz")
                .await
                .status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[tokio::test]
    async fn worker_edge_caches_internal_npm_across_infra_restart() {
        let directory = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let gateway = format!("http://{address}");
        let package = PackageIdentity::new(PackageEcosystem::Npm, "@goobits/auth").unwrap();
        let catalog = InternalPackageCatalog::new([package]);
        let tarball_url = format!("{gateway}/npm/%40goobits%2Fauth/-/auth-1.0.0.tgz");
        let infra = Router::new()
            .route(
                "/work/v1/catalog",
                get(move || {
                    let catalog = catalog.clone();
                    async move { Json(catalog) }
                }),
            )
            .route(
                "/npm/{package}",
                get(move || {
                    let tarball_url = tarball_url.clone();
                    async move {
                        Json(serde_json::json!({
                            "name": "@goobits/auth",
                            "versions": {
                                "1.0.0": {
                                    "name": "@goobits/auth",
                                    "version": "1.0.0",
                                    "dist": { "tarball": tarball_url }
                                }
                            }
                        }))
                    }
                }),
            )
            .route(
                "/npm/{package}/-/{filename}",
                get(|| async { b"internal npm archive".to_vec() }),
            );
        let infra_task = tokio::spawn(async move {
            axum::serve(listener, infra).await.unwrap();
        });

        let (edge_gateway, edge_task) =
            spawn_http(app_router(edge_state(directory.path(), &gateway))).await;
        let http = reqwest::Client::new();
        let metadata_url = format!("{edge_gateway}/npm/%40goobits%2Fauth");
        let metadata: serde_json::Value = http
            .get(&metadata_url)
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        let edge_tarball = metadata["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert!(edge_tarball.starts_with(&edge_gateway));
        let archive = http
            .get(edge_tarball)
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(&archive[..], b"internal npm archive");

        infra_task.abort();
        edge_task.abort();
        let _ = infra_task.await;
        let _ = edge_task.await;
        let (restarted_gateway, restarted_edge) =
            spawn_http(app_router(edge_state(directory.path(), &gateway))).await;
        let metadata = http
            .get(format!("{restarted_gateway}/npm/%40goobits%2Fauth"))
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        let tarball = metadata["versions"]["1.0.0"]["dist"]["tarball"]
            .as_str()
            .unwrap();
        assert!(tarball.starts_with(&restarted_gateway));
        let archive = http
            .get(tarball)
            .bearer_auth("read")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .bytes()
            .await
            .unwrap();
        assert_eq!(&archive[..], b"internal npm archive");
        restarted_edge.abort();
    }
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn status_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let mut info = HashMap::new();
    info.insert("status", "ok");
    info.insert("service", "goobits-pkg-server");
    info.insert("version", env!("CARGO_PKG_VERSION"));

    let data_dir_str = state.data_dir.to_string_lossy();
    let data_dir_info = format!("Using data directory: {data_dir_str}");

    let response = format!(
        r#"{{
  "status": "ok",
  "service": "goobits-pkg-server",
  "version": "{}",
  "data_directory": "{}",
  "registries": ["npm", "pypi", "cargo"]
}}"#,
        env!("CARGO_PKG_VERSION"),
        data_dir_str
    );

    info!("{}", data_dir_info);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .expect("static header value is valid"),
    );

    (StatusCode::OK, headers, response)
}

async fn health_handler() -> impl IntoResponse {
    let response = r#"{"status": "healthy"}"#;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/json"
            .parse()
            .expect("static header value is valid"),
    );

    (StatusCode::OK, headers, response)
}

async fn list_packages_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Pass data directory directly to avoid thread-unsafe directory changes
    let data_dir = state.data_dir.clone();
    let result =
        tokio::task::spawn_blocking(move || crate::local_storage::list_local_packages(&data_dir))
            .await;

    match result {
        Ok(inner_result) => match inner_result {
            Ok(packages) => {
                let json = serde_json::to_string(&packages).unwrap_or_else(|_| "{}".to_string());
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    "application/json"
                        .parse()
                        .expect("static header value is valid"),
                );
                (StatusCode::OK, headers, json)
            }
            Err(e) => {
                error!("Failed to list packages: {}", e);
                let error_response = format!(r#"{{"error": "{e}"}}"#);
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    "application/json"
                        .parse()
                        .expect("static header value is valid"),
                );
                (StatusCode::INTERNAL_SERVER_ERROR, headers, error_response)
            }
        },
        Err(e) => {
            error!("Task join error: {}", e);
            let error_response = r#"{"error": "Internal Server Error"}"#.to_string();
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "application/json"
                    .parse()
                    .expect("static header value is valid"),
            );
            (StatusCode::INTERNAL_SERVER_ERROR, headers, error_response)
        }
    }
}

#[derive(Deserialize)]
struct SetupQuery {
    registry: Option<String>,
    port: Option<u16>,
}

async fn setup_script_handler(Query(params): Query<SetupQuery>) -> Response {
    let registry = params.registry.as_deref().unwrap_or("npm");
    let port = params.port.unwrap_or(8080);

    let script = serve_setup_script(registry, port);

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/plain".parse().expect("static header value is valid"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"setup.sh\""
            .parse()
            .expect("static header value is valid"),
    );

    (StatusCode::OK, headers, script).into_response()
}

fn serve_setup_script(registry: &str, port: u16) -> String {
    let server_url = format!("http://$(hostname -I | cut -d' ' -f1):{port}");

    match registry {
        "npm" => format!(
            r#"#!/bin/bash
# Goobits Package Server - NPM Setup Script
# This script configures npm to use your private package registry

echo "🔧 Configuring npm to use Goobits Package Server..."
echo "📡 Registry URL: {server_url}/npm/"

# Set npm registry
npm config set registry {server_url}/npm/

echo "✅ npm configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   npm whoami          # Check current user"
echo "   npm config list     # View configuration"
echo "   npm config set registry https://registry.npmjs.org/  # Reset to default"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#
        ),
        "pypi" => format!(
            r#"#!/bin/bash
# Goobits Package Server - PyPI Setup Script
# This script configures pip to use your private package registry

echo "🔧 Configuring pip to use Goobits Package Server..."
echo "📡 Registry URL: {server_url}/pypi/simple/"

# Create pip config directory
mkdir -p ~/.config/pip
mkdir -p ~/.pip

# Configure pip
cat > ~/.config/pip/pip.conf << EOF
[global]
index-url = {server_url}/pypi/simple/
trusted-host = $(echo {server_url} | cut -d'/' -f3 | cut -d':' -f1)
EOF

# Also create old-style config for compatibility
cat > ~/.pip/pip.conf << EOF
[global]
index-url = {server_url}/pypi/simple/
trusted-host = $(echo {server_url} | cut -d'/' -f3 | cut -d':' -f1)
EOF

echo "✅ pip configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   pip config list     # View configuration"
echo "   pip install --index-url https://pypi.org/simple/ <package>  # Install from PyPI"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#
        ),
        "cargo" => format!(
            r#"#!/bin/bash
# Goobits Package Server - Cargo Setup Script
# This script configures cargo to use your private package registry

echo "🔧 Configuring cargo to use Goobits Package Server..."
echo "📡 Registry URL: {server_url}/cargo/"

# Create cargo config directory
mkdir -p ~/.cargo

# Configure cargo
cat > ~/.cargo/config.toml << EOF
[registries]
goobits = {{ index = "{server_url}/cargo/" }}

[source.crates-io]
replace-with = "goobits"

[source.goobits]
registry = "{server_url}/cargo/"
EOF

echo "✅ cargo configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   cargo search <package>    # Search for packages"
echo "   cargo install <package>   # Install a package"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#
        ),
        _ => {
            warn!(registry = %registry, "Unknown registry type requested");
            format!(
                r#"#!/bin/bash
# Goobits Package Server - Setup Script
# Unknown registry type: {registry}

echo "❌ Unknown registry type: {registry}"
echo "📋 Supported registries: npm, pypi, cargo"
echo ""
echo "🔧 Usage examples:"
echo "   curl {server_url}/setup.sh?registry=npm | bash"
echo "   curl {server_url}/setup.sh?registry=pypi | bash"
echo "   curl {server_url}/setup.sh?registry=cargo | bash"
"#
            )
        }
    }
}
