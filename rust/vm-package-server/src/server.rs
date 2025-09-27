//! HTTP server setup and route handlers for package registry operations
//!
//! This module contains the complete Axum-based HTTP server implementation
//! supporting npm, PyPI, and Cargo package registry operations.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::Result;
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, put},
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
    info!("Starting Goobits Package Server in background");
    println!("🚀 Starting Goobits Package Server in background...");

    if let Err(e) = validation::validate_hostname(&host) {
        error!(host = %host, error = %e, "Invalid host parameter");
        eprintln!("❌ Invalid host parameter: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = validation::validate_docker_port(port) {
        error!(port = %port, error = %e, "Invalid port parameter");
        eprintln!("❌ Invalid port parameter: {}", e);
        std::process::exit(1);
    }

    let abs_data_dir = match std::fs::canonicalize(&data_dir) {
        Ok(path) => path,
        Err(_) => {
            std::fs::create_dir_all(&data_dir)?;
            match std::env::current_dir() {
                Ok(current) => current.join(&data_dir),
                Err(e) => {
                    error!(error = %e, "Failed to get current directory");
                    eprintln!("❌ Failed to get current directory: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    info!(data_dir = %abs_data_dir.display(), "Using data directory");
    info!(host = %host, port = %port, "Starting background server");
    println!("📂 Using data directory: {}", abs_data_dir.display());

    let pid_file = abs_data_dir.join("server.pid");
    let log_file = abs_data_dir.join("server.log");

    let current_exe = std::env::current_exe().map_err(|e| {
        error!(error = %e, "Failed to get current executable path");
        anyhow::anyhow!("Failed to get current executable path: {}", e)
    })?;

    let mut cmd = Command::new(&current_exe);
    cmd.args([
        "start",
        "--host",
        &host,
        "--port",
        &port.to_string(),
        "--data",
        &abs_data_dir.to_string_lossy(),
    ]);

    cmd.stdout(Stdio::null()).stderr(Stdio::null());

    let child = cmd.spawn().map_err(|e| {
        error!(error = %e, "Failed to spawn background process");
        anyhow::anyhow!("Failed to spawn background process: {}", e)
    })?;

    std::fs::write(&pid_file, child.id().to_string()).map_err(|e| {
        error!(error = %e, pid_file = %pid_file.display(), "Failed to write PID file");
        anyhow::anyhow!("Failed to write PID file: {}", e)
    })?;

    println!("✅ Server started in background with PID: {}", child.id());
    println!("📄 PID file: {}", pid_file.display());
    println!("📋 Log file: {}", log_file.display());
    println!();
    println!("🌐 Server will be accessible at:");
    println!("   Local:      http://localhost:{}", port);
    println!("   Network:    http://<your-ip>:{}", port);
    println!();
    println!("🔧 Configure other machines:");
    println!("   curl http://<your-ip>:{}/setup.sh | bash", port);
    println!();
    println!("📋 Management commands:");
    println!("   Stop:       goobits-pkg-server stop");
    println!("   Status:     goobits-pkg-server status");
    println!("   Logs:       tail -f {}", log_file.display());

    Ok(())
}

pub async fn run_server(host: String, port: u16, data_dir: PathBuf) -> Result<()> {
    info!("Starting Goobits Package Server");
    println!("🚀 Starting Goobits Package Server...");

    if let Err(e) = validation::validate_hostname(&host) {
        error!(host = %host, error = %e, "Invalid host parameter");
        eprintln!("❌ Invalid host parameter: {}", e);
        std::process::exit(1);
    }

    if let Err(e) = validation::validate_docker_port(port) {
        error!(port = %port, error = %e, "Invalid port parameter");
        eprintln!("❌ Invalid port parameter: {}", e);
        std::process::exit(1);
    }

    let abs_data_dir = match std::fs::canonicalize(&data_dir) {
        Ok(path) => path,
        Err(_) => {
            std::fs::create_dir_all(&data_dir)?;
            match std::env::current_dir() {
                Ok(current) => current.join(&data_dir),
                Err(e) => {
                    error!(error = %e, "Failed to get current directory");
                    eprintln!("❌ Failed to get current directory: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    info!(data_dir = %abs_data_dir.display(), "Using data directory");
    info!(host = %host, port = %port, "Starting server");
    println!("📂 Using data directory: {}", abs_data_dir.display());

    // Create required components for AppState
    let upstream_config = UpstreamConfig::default();
    let upstream_client = Arc::new(UpstreamClient::new(upstream_config)?);
    let config = Arc::new(Config::default());
    let server_addr = format!("http://{}:{}", host, port);

    let state = AppState {
        data_dir: abs_data_dir,
        server_addr,
        upstream_client,
        config,
    };

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/status", get(status_handler))
        .route("/setup.sh", get(setup_script_handler))
        .route("/health", get(health_handler))
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
        .route("/cargo/api/v1/crates", put(cargo::publish_crate))
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

    let addr: SocketAddr = format!("{}:{}", host, port).parse().map_err(|e| {
        error!(host = %host, port = %port, error = %e, "Invalid socket address");
        anyhow::anyhow!("Invalid socket address {}:{}: {}", host, port, e)
    })?;

    let listener = TcpListener::bind(&addr).await.map_err(|e| {
        error!(addr = %addr, error = %e, "Failed to bind to address");
        anyhow::anyhow!("Failed to bind to {}:{}: {}", host, port, e)
    })?;

    println!("✅ Server is running on http://{}:{}", host, port);
    println!();
    println!("🌐 Server is accessible at:");
    println!("   Local:      http://localhost:{}", port);
    println!("   Network:    http://<your-ip>:{}", port);
    println!();
    println!("🔧 Configure other machines:");
    println!("   curl http://<your-ip>:{}/setup.sh | bash", port);
    println!();
    println!("📋 Quick commands:");
    println!("   Status:     curl http://localhost:{}/status", port);
    println!("   Health:     curl http://localhost:{}/health", port);
    println!("   Setup:      curl http://localhost:{}/setup.sh", port);

    info!("Server listening on {}", addr);
    axum::serve(listener, app).await.map_err(|e| {
        error!(error = %e, "Server error");
        anyhow::anyhow!("Server error: {}", e)
    })?;

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
    let data_dir_info = format!("Using data directory: {}", data_dir_str);

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
    let server_url = format!("http://$(hostname -I | cut -d' ' -f1):{}", port);

    match registry {
        "npm" => format!(
            r#"#!/bin/bash
# Goobits Package Server - NPM Setup Script
# This script configures npm to use your private package registry

echo "🔧 Configuring npm to use Goobits Package Server..."
echo "📡 Registry URL: {}/npm/"

# Set npm registry
npm config set registry {}/npm/

echo "✅ npm configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   npm whoami          # Check current user"
echo "   npm config list     # View configuration"
echo "   npm config set registry https://registry.npmjs.org/  # Reset to default"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#,
            server_url, server_url
        ),
        "pypi" => format!(
            r#"#!/bin/bash
# Goobits Package Server - PyPI Setup Script
# This script configures pip to use your private package registry

echo "🔧 Configuring pip to use Goobits Package Server..."
echo "📡 Registry URL: {}/pypi/simple/"

# Create pip config directory
mkdir -p ~/.config/pip
mkdir -p ~/.pip

# Configure pip
cat > ~/.config/pip/pip.conf << EOF
[global]
index-url = {}/pypi/simple/
trusted-host = $(echo {} | cut -d'/' -f3 | cut -d':' -f1)
EOF

# Also create old-style config for compatibility
cat > ~/.pip/pip.conf << EOF
[global]
index-url = {}/pypi/simple/
trusted-host = $(echo {} | cut -d'/' -f3 | cut -d':' -f1)
EOF

echo "✅ pip configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   pip config list     # View configuration"
echo "   pip install --index-url https://pypi.org/simple/ <package>  # Install from PyPI"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#,
            server_url, server_url, server_url, server_url, server_url
        ),
        "cargo" => format!(
            r#"#!/bin/bash
# Goobits Package Server - Cargo Setup Script
# This script configures cargo to use your private package registry

echo "🔧 Configuring cargo to use Goobits Package Server..."
echo "📡 Registry URL: {}/cargo/"

# Create cargo config directory
mkdir -p ~/.cargo

# Configure cargo
cat > ~/.cargo/config.toml << EOF
[registries]
goobits = {{ index = "{}/cargo/" }}

[source.crates-io]
replace-with = "goobits"

[source.goobits]
registry = "{}/cargo/"
EOF

echo "✅ cargo configured successfully!"
echo ""
echo "📋 Useful commands:"
echo "   cargo search <package>    # Search for packages"
echo "   cargo install <package>   # Install a package"
echo ""
echo "🚀 You can now install packages from your private registry!"
"#,
            server_url, server_url, server_url
        ),
        _ => {
            warn!(registry = %registry, "Unknown registry type requested");
            format!(
                r#"#!/bin/bash
# Goobits Package Server - Setup Script
# Unknown registry type: {}

echo "❌ Unknown registry type: {}"
echo "📋 Supported registries: npm, pypi, cargo"
echo ""
echo "🔧 Usage examples:"
echo "   curl {}/setup.sh?registry=npm | bash"
echo "   curl {}/setup.sh?registry=pypi | bash"
echo "   curl {}/setup.sh?registry=cargo | bash"
"#,
                registry, registry, server_url, server_url, server_url
            )
        }
    }
}
