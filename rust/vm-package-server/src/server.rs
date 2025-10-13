//! HTTP server setup and route handlers for package registry operations
//!
//! This module contains the complete Axum-based HTTP server implementation
//! supporting npm, PyPI, and Cargo package registry operations.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

use crate::validation;
use crate::{
    cargo,
    config::Config,
    npm, pypi,
    state::AppState,
    upstream::{UpstreamClient, UpstreamConfig},
};

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

    if let Err(e) = validation::validate_hostname(&host) {
        error!(host = %host, error = %e, "❌ Invalid host parameter: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = validation::validate_docker_port(port) {
        error!(port = %port, error = %e, "❌ Invalid port parameter: {}", e);
        std::process::exit(1);
    }

    let abs_data_dir = match std::fs::canonicalize(&data_dir) {
        Ok(path) => path,
        Err(_) => {
            std::fs::create_dir_all(&data_dir)?;
            match std::env::current_dir() {
                Ok(current) => current.join(&data_dir),
                Err(e) => {
                    error!(error = %e, "❌ Failed to get current directory: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    info!(data_dir = %abs_data_dir.display(), "📂 Using data directory: {}", abs_data_dir.display());
    info!(host = %host, port = %port, "Starting server");

    // Create required components for AppState
    let upstream_config = UpstreamConfig::default();
    let upstream_client = Arc::new(UpstreamClient::new(upstream_config)?);
    let config = Arc::new(Config::default());
    let server_addr = format!("http://{host}:{port}");

    let state = AppState {
        data_dir: abs_data_dir,
        server_addr,
        upstream_client,
        config,
    };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/status", get(status_handler))
        .route("/api/status", get(status_handler))
        .route("/setup.sh", get(setup_script_handler))
        .route("/health", get(health_handler))
        .route("/api/packages", get(list_packages_handler))
        .route("/shutdown", post(shutdown_handler))
        .route("/npm/{package}", put(npm::publish_package))
        .route("/npm/{package}/-/{filename}", get(npm::download_tarball))
        .route("/npm/{package}", get(npm::package_metadata))
        .route("/pypi/simple/{package}/", get(pypi::package_index))
        .route("/pypi/packages/{filename}", get(pypi::download_file))
        .route("/pypi/legacy/api/pypi", get(pypi::simple_index))
        .route("/pypi/legacy/api/pypi/{package}/", get(pypi::package_index))
        .route(
            "/pypi/legacy/api/pypi/{package}/{version}",
            get(pypi::package_index),
        )
        .route("/pypi/upload", put(pypi::upload_package))
        .route("/cargo/api/v1/crates/new", put(cargo::publish_crate))
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
        .with_state(Arc::new(state));

    let addr: SocketAddr = format!("{host}:{port}").parse().map_err(|e| {
        error!(host = %host, port = %port, error = %e, "Invalid socket address");
        anyhow::anyhow!("Invalid socket address {host}:{port}: {e}")
    })?;

    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        error!(addr = %addr, error = %e, "Failed to bind to address");
        anyhow::anyhow!("Failed to bind to {host}:{port}: {e}")
    })?;

    info!("✅ Server is running on http://{}:{}", host, port);
    info!("🌐 Server is accessible at:");
    info!("   Local:      http://localhost:{}", port);
    info!("   Network:    http://<your-ip>:{}", port);
    info!("");
    info!("🔧 Configure other machines:");
    info!("   curl http://<your-ip>:{}/setup.sh | bash", port);
    info!("");
    info!("📋 Quick commands:");
    info!("   Status:     curl http://localhost:{}/status", port);
    info!("   Health:     curl http://localhost:{}/health", port);
    info!("   Setup:      curl http://localhost:{}/setup.sh", port);
    info!("Server listening on {}", addr);

    match shutdown_receiver {
        Some(shutdown_rx) => {
            // Use graceful shutdown when shutdown receiver is provided
            axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    shutdown_rx.await.ok();
                    info!("Received shutdown signal, stopping server gracefully");
                })
                .await
                .map_err(|e| {
                    error!(error = %e, "Server error");
                    anyhow::anyhow!("Server error: {e}")
                })?;
        }
        None => {
            // Original behavior - run indefinitely
            axum::serve(listener, app).await.map_err(|e| {
                error!(error = %e, "Server error");
                anyhow::anyhow!("Server error: {e}")
            })?;
        }
    }

    Ok(())
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
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

    (StatusCode::OK, headers, response)
}

async fn health_handler() -> impl IntoResponse {
    let response = r#"{"status": "healthy"}"#;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

    (StatusCode::OK, headers, response)
}

async fn list_packages_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    // Pass data directory directly to avoid thread-unsafe directory changes
    let result = crate::local_storage::list_local_packages(&state.data_dir);

    match result {
        Ok(packages) => {
            let json = serde_json::to_string(&packages).unwrap_or_else(|_| "{}".to_string());
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            (StatusCode::OK, headers, json)
        }
        Err(e) => {
            error!("Failed to list packages: {}", e);
            let error_response = format!(r#"{{"error": "{e}"}}"#);
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());
            (StatusCode::INTERNAL_SERVER_ERROR, headers, error_response)
        }
    }
}

async fn shutdown_handler() -> impl IntoResponse {
    info!("Shutdown endpoint called");
    let response = r#"{"status": "shutdown_initiated"}"#;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/json".parse().unwrap());

    (StatusCode::OK, headers, response)
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
    headers.insert(header::CONTENT_TYPE, "text/plain".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"setup.sh\"".parse().unwrap(),
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
