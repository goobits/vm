use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Path as AxumPath, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Json},
    routing::{get, post, put},
    Router,
};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing::{error, info};

mod api;
mod cargo;
mod cli;
mod client_ops;
mod deletion;
mod docker_manager;
mod live_reload;
mod npm;
mod pypi;
mod types;
mod ui;
mod utils;
mod validation;
mod wrapper;

use vm_package_server::registry_trait::get_registry;
use vm_package_server::{auth, config::Config, AppState, UpstreamClient, UpstreamConfig};

use types::{PackageName, Registry, Version};

// Re-export Docker functionality
pub use docker_manager::run_in_docker;

/// Helper function to validate package name and version parameters
fn validate_package_params(
    package_name: String,
    version: Option<String>,
) -> Result<(PackageName, Option<Version>), vm_package_server::AppError> {
    let package = PackageName::new(package_name).map_err(|e| {
        vm_package_server::AppError::BadRequest(format!("Invalid package name: {}", e))
    })?;

    let version = if let Some(v) = version {
        Some(Version::new(v).map_err(|e| {
            vm_package_server::AppError::BadRequest(format!("Invalid version: {}", e))
        })?)
    } else {
        None
    };

    Ok((package, version))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    cli::run().await
}

/// Run the server in background mode
async fn run_server_background(host: String, port: u16, data: PathBuf) -> anyhow::Result<()> {
    // Create a PID file location
    let pid_file_path = data.join(".pkg-server.pid");

    // Check if server is already running
    if pid_file_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                // Check if process is still running
                let check = Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();

                if check.map(|s| s.success()).unwrap_or(false) {
                    println!("❌ Server is already running (PID: {})", pid);
                    println!("   Stop it first with: pkg-server stop");
                    return Ok(());
                }
            }
        }
    }

    // Start the server in background using tokio spawn
    let exe_path = std::env::current_exe()?;
    let log_file_path = data.join("server.log");

    // Prepare arguments for the foreground server
    let args = vec![
        "start".to_string(),
        "--foreground".to_string(),
        "--host".to_string(),
        host.clone(),
        "--port".to_string(),
        port.to_string(),
        "--data".to_string(),
        data.to_string_lossy().to_string(),
        "--no-config".to_string(), // Already configured
    ];

    // Create log file for output redirection
    let log_file = File::create(&log_file_path)?;

    // Spawn the background process
    let child = Command::new(exe_path)
        .args(&args)
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .stdin(Stdio::null())
        .spawn()?;

    // Save the child PID to file
    let pid = child.id();
    let mut pid_file = File::create(&pid_file_path)?;
    writeln!(pid_file, "{}", pid)?;

    // Detach the child process so it continues running independently
    // Note: The child process will become orphaned when parent exits
    std::mem::forget(child);

    println!("✅ Server started in background mode");
    println!();
    println!("🌐 Server will be accessible at:");
    println!("   Local:      http://localhost:{}", port);
    println!("   Network:    http://{}:{}", host, port);
    println!();
    println!("📋 Commands:");
    println!("   Stop server:  pkg-server stop");
    println!("   View status:  pkg-server status");
    println!("   View logs:    tail -f {}", log_file_path.display());
    println!();
    println!("💡 To run in foreground mode (for debugging):");
    println!("   pkg-server start --foreground");

    Ok(())
}

async fn run_server(host: String, port: u16, data: PathBuf) -> anyhow::Result<()> {
    // Create data directories
    tokio::fs::create_dir_all(&data).await?;
    tokio::fs::create_dir_all(data.join("pypi/packages")).await?;
    tokio::fs::create_dir_all(data.join("npm/tarballs")).await?;
    tokio::fs::create_dir_all(data.join("npm/metadata")).await?;
    tokio::fs::create_dir_all(data.join("cargo/crates")).await?;
    tokio::fs::create_dir_all(data.join("cargo/index")).await?;
    // Create static directory if it doesn't exist
    tokio::fs::create_dir_all("static").await?;

    info!(data_dir = %data.display(), "Created data directories");

    let addr = format!("{}:{}", host, port);

    // Load user configuration for registry fallback settings
    let user_config = vm_package_server::user_config::UserConfig::load().unwrap_or_default();

    // Create upstream client with user configuration
    let upstream_config = UpstreamConfig {
        enabled: user_config.registry.pypi_fallback
            || user_config.registry.npm_fallback
            || user_config.registry.cargo_fallback,
        timeout: if user_config.registry.cache_ttl > 0 {
            std::time::Duration::from_secs(user_config.registry.cache_ttl)
        } else {
            UpstreamConfig::default().timeout
        },
        ..Default::default()
    };

    let upstream_client = Arc::new(UpstreamClient::new(upstream_config)?);

    // Load application config
    let config = Arc::new(Config::load_or_default("config.json")?);

    // Log authentication status
    if config.security.require_authentication {
        info!(
            "Authentication enabled - {} API keys configured",
            config.security.api_keys.len()
        );
    } else {
        info!("Authentication disabled - all uploads are open");
    }

    let state = Arc::new(AppState {
        data_dir: data.clone(),
        server_addr: format!("http://{}", addr),
        upstream_client,
        config: config.clone(),
    });

    // Initialize live reload state
    let live_reload_state = Arc::new(live_reload::LiveReloadState::new());

    // Start file watcher for live reload (only in development)
    #[cfg(debug_assertions)]
    {
        if let Err(e) = live_reload::start_file_watcher(live_reload_state.clone()) {
            error!("Failed to start live reload watcher: {}", e);
        }
    }

    // Build our application with routes and size limits
    let app = Router::new()
        // PyPI routes
        .route("/pypi/simple/", get(pypi::simple_index))
        .route("/pypi/simple/{package}/", get(pypi::package_index))
        .route("/pypi/packages/{filename}", get(pypi::download_file))
        .route(
            "/pypi/",
            post(pypi::upload_package)
                .layer(DefaultBodyLimit::max(validation::MAX_REQUEST_BODY_SIZE))
                .layer(middleware::from_fn_with_state(
                    config.clone(),
                    auth::auth_middleware,
                )),
        )
        // Version-specific deletion routes (via registry trait wrappers)
        .route(
            "/pypi/{package_name}/{version}",
            axum::routing::delete(pypi_delete_package_version),
        )
        .route(
            "/pypi/package/{package_name}",
            axum::routing::delete(pypi_delete_all_versions),
        )
        // npm routes
        .route("/npm/", get(npm_registry_root))
        .route("/npm/{package}", get(npm::package_metadata))
        .route("/npm/{package}/-/{filename}", get(npm::download_tarball))
        .route(
            "/npm/{package}",
            put(npm::publish_package)
                .layer(DefaultBodyLimit::max(validation::MAX_REQUEST_BODY_SIZE))
                .layer(middleware::from_fn_with_state(
                    config.clone(),
                    auth::auth_middleware,
                )),
        )
        .route(
            "/npm/{package_name}/{version}",
            axum::routing::delete(npm_delete_package_version),
        )
        .route(
            "/npm/package/{package_name}",
            axum::routing::delete(npm_delete_all_versions),
        )
        // Cargo routes
        .route("/cargo/", get(cargo_registry_root))
        .route("/cargo/config.json", get(cargo::config))
        .route(
            "/cargo/api/v1/crates/{crate}/{version}/download",
            get(cargo::download_crate),
        )
        .route(
            "/cargo/api/v1/crates/new",
            put(cargo::publish_crate)
                .layer(DefaultBodyLimit::max(validation::MAX_REQUEST_BODY_SIZE))
                .layer(middleware::from_fn_with_state(
                    config.clone(),
                    auth::auth_middleware,
                )),
        )
        .route(
            "/api/v1/cargo/{crate_name}/versions",
            get(cargo::get_crate_versions_api),
        )
        .route(
            "/api/cargo/{crate_name}/{version}",
            axum::routing::delete(cargo::delete_crate_version),
        )
        .route(
            "/api/cargo/crate/{crate_name}",
            axum::routing::delete(cargo::delete_all_versions),
        )
        // Sparse index routes for Cargo (HTTP-based)
        .route(
            "/cargo/api/v1/crates",
            get(cargo::crates_io_api_placeholder),
        )
        .route("/cargo/{*path}", get(cargo::sparse_index))
        // API routes for listing and status
        .route("/api/packages", get(list_all_packages_api))
        .route("/api/status", get(server_status_api))
        // Generic registry API routes (trait-driven)
        .route("/api/{registry}/packages/count", get(count_packages_api))
        .route("/api/{registry}/packages", get(list_packages_api))
        .route(
            "/api/{registry}/packages/{package_name}/{version}",
            axum::routing::delete(delete_package_version_api),
        )
        .route(
            "/api/{registry}/packages/{package_name}",
            axum::routing::delete(delete_all_versions_api),
        )
        .route("/setup.sh", get(serve_setup_script))
        // UI routes
        .route("/", get(ui::home))
        .route("/upload", get(ui::upload_page))
        .route("/ui/{pkg_type}", get(ui::list_packages))
        .route("/ui/pypi/{pkg_name}", get(ui::pypi_package_detail))
        .route("/ui/npm/{pkg_name}", get(ui::npm_package_detail))
        .route("/ui/cargo/{pkg_name}", get(ui::cargo_package_detail))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        // Serve static files from the static directory
        .fallback_service(ServeDir::new("static"));

    // Add WebSocket route for live reload as a separate router
    let app = Router::new()
        .route("/ws/reload", get(live_reload::websocket_handler))
        .with_state(live_reload_state)
        .fallback_service(app);

    let listener = TcpListener::bind(&addr).await?;

    info!(
        addr = %addr,
        data_dir = %data.display(),
        pypi_dir = %data.join("pypi/packages").display(),
        npm_dir = %data.join("npm").display(),
        cargo_dir = %data.join("cargo").display(),
        "Server started successfully"
    );

    axum::serve(listener, app).await?;
    Ok(())
}

/// API endpoint to list all packages across ecosystems (now using trait system)
async fn list_all_packages_api(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HashMap<String, Vec<String>>>, vm_package_server::AppError> {
    let mut packages = HashMap::new();

    // Use trait system for all registries
    for registry_name in &["pypi", "npm", "cargo"] {
        let registry = get_registry(registry_name)?;
        let registry_packages = registry.list_all_packages(&state).await.unwrap_or_default();
        packages.insert(registry_name.to_string(), registry_packages);
    }

    Ok(Json(packages))
}

/// API endpoint for server status
async fn server_status_api(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, vm_package_server::AppError> {
    let status = json!({
        "status": "running",
        "server_addr": state.server_addr,
        "data_dir": state.data_dir,
        "version": env!("CARGO_PKG_VERSION")
    });

    Ok(Json(status))
}

/// Generic API endpoint for counting packages by registry
async fn count_packages_api(
    AxumPath(registry_name): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, vm_package_server::AppError> {
    let registry = get_registry(&registry_name)?;
    let count = registry.count_packages(&state).await?;

    Ok(Json(json!({
        "registry": registry_name,
        "count": count
    })))
}

/// Generic API endpoint for listing packages by registry
async fn list_packages_api(
    AxumPath(registry_name): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, vm_package_server::AppError> {
    let registry = get_registry(&registry_name)?;
    let packages = registry.list_all_packages(&state).await?;

    Ok(Json(json!({
        "registry": registry_name,
        "packages": packages
    })))
}

async fn registry_delete_package_version(
    state: &Arc<AppState>,
    registry_name: &str,
    package_name: &str,
    version: &str,
) -> vm_package_server::AppResult<vm_package_server::SuccessResponse> {
    let registry = get_registry(registry_name)?;
    registry
        .delete_package_version(state.as_ref(), package_name, version)
        .await
}

async fn registry_delete_all_versions(
    state: &Arc<AppState>,
    registry_name: &str,
    package_name: &str,
) -> vm_package_server::AppResult<vm_package_server::SuccessResponse> {
    let registry = get_registry(registry_name)?;
    registry
        .delete_all_versions(state.as_ref(), package_name)
        .await
}

/// Generic API endpoint for deleting package version by registry
async fn delete_package_version_api(
    AxumPath((registry_name, package_name, version)): AxumPath<(String, String, String)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<vm_package_server::SuccessResponse>, vm_package_server::AppError> {
    // Parse and validate strong types
    let registry = registry_name
        .parse::<Registry>()
        .map_err(|e| vm_package_server::AppError::BadRequest(format!("Invalid registry: {}", e)))?;

    let (package, version) = validate_package_params(package_name, Some(version))?;
    let version = version.unwrap(); // Safe because we passed Some(version)

    let response = registry_delete_package_version(
        &state,
        registry.as_str(),
        package.as_str(),
        version.as_str(),
    )
    .await?;
    Ok(Json(response))
}

/// Generic API endpoint for deleting all package versions by registry
async fn delete_all_versions_api(
    AxumPath((registry_name, package_name)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<vm_package_server::SuccessResponse>, vm_package_server::AppError> {
    // Parse and validate strong types
    let registry = registry_name
        .parse::<Registry>()
        .map_err(|e| vm_package_server::AppError::BadRequest(format!("Invalid registry: {}", e)))?;

    let (package, _) = validate_package_params(package_name, None)?;

    let response =
        registry_delete_all_versions(&state, registry.as_str(), package.as_str()).await?;
    Ok(Json(response))
}

/// PyPI-specific delete package version handler using trait system
async fn pypi_delete_package_version(
    AxumPath((package_name, version)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<vm_package_server::SuccessResponse>, vm_package_server::AppError> {
    let (package, version) = validate_package_params(package_name, Some(version))?;
    let version = version.unwrap(); // Safe because we passed Some(version)

    let response =
        registry_delete_package_version(&state, "pypi", package.as_str(), version.as_str()).await?;
    Ok(Json(response))
}

/// PyPI-specific delete all versions handler using trait system
async fn pypi_delete_all_versions(
    AxumPath(package_name): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<vm_package_server::SuccessResponse>, vm_package_server::AppError> {
    let (package, _) = validate_package_params(package_name, None)?;

    let response = registry_delete_all_versions(&state, "pypi", package.as_str()).await?;
    Ok(Json(response))
}

/// NPM-specific delete package version handler using trait system
async fn npm_delete_package_version(
    AxumPath((package_name, version)): AxumPath<(String, String)>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<vm_package_server::SuccessResponse>, vm_package_server::AppError> {
    let (package, version) = validate_package_params(package_name, Some(version))?;
    let version = version.unwrap(); // Safe because we passed Some(version)

    let response =
        registry_delete_package_version(&state, "npm", package.as_str(), version.as_str()).await?;
    Ok(Json(response))
}

/// NPM-specific delete all versions handler using trait system
async fn npm_delete_all_versions(
    AxumPath(package_name): AxumPath<String>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<vm_package_server::SuccessResponse>, vm_package_server::AppError> {
    let (package, _) = validate_package_params(package_name, None)?;

    let response = registry_delete_all_versions(&state, "npm", package.as_str()).await?;
    Ok(Json(response))
}

/// NPM registry root endpoint - returns empty JSON object like npmjs.org
async fn npm_registry_root() -> Json<Value> {
    Json(json!({}))
}

/// Cargo registry root endpoint - returns basic registry info
async fn cargo_registry_root() -> Json<Value> {
    Json(json!({
        "api": "/cargo/api/v1",
        "dl": "/cargo/api/v1/crates"
    }))
}

/// Validate server IP/hostname to prevent injection attacks
fn validate_server_address(server_ip: &str) -> bool {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    // Check basic length constraints
    if server_ip.is_empty() || server_ip.len() > 253 {
        return false;
    }

    // Check for null bytes or control characters
    if server_ip.contains('\0') || server_ip.chars().any(|c| c.is_control()) {
        return false;
    }

    // Try to parse as IP address first
    if server_ip.parse::<IpAddr>().is_ok() {
        return true;
    }

    // Try to parse IPv4 specifically
    if server_ip.parse::<Ipv4Addr>().is_ok() {
        return true;
    }

    // Try to parse IPv6 specifically (handles bracketed format)
    let ipv6_candidate = if server_ip.starts_with('[') && server_ip.ends_with(']') {
        &server_ip[1..server_ip.len() - 1]
    } else {
        server_ip
    };
    if ipv6_candidate.parse::<Ipv6Addr>().is_ok() {
        return true;
    }

    // Validate as hostname using our validation module
    validation::validate_hostname(server_ip).is_ok()
}

/// Create the setup script content for configuring package managers
fn create_setup_script_content(server_ip: &str, port: u16) -> String {
    // Validate inputs to prevent shell injection
    if !validate_server_address(server_ip) {
        error!(server_ip = %server_ip, "Invalid server address format");
        eprintln!("Error: Invalid server address format");
        std::process::exit(1);
    }

    if port == 0 {
        error!(port = %port, "Invalid port number");
        eprintln!("Error: Invalid port number");
        std::process::exit(1);
    }

    // Safely escape the server IP for shell usage
    let safe_server_ip = validation::escape_shell_arg(server_ip);
    let safe_port = port.to_string();

    format!(
        r#"#!/bin/bash
# Goobits Package Server - Client Setup Script
# Generated for server at {safe_server_ip}:{safe_port}

SERVER_URL="http://{safe_server_ip}:{safe_port}"

echo "🚀 Configuring package managers to use Goobits Package Server at $SERVER_URL"
echo ""

# Configure pip
configure_pip() {{
    echo "📦 Configuring pip..."
    mkdir -p ~/.pip
    cat > ~/.pip/pip.conf << EOF
[global]
index-url = $SERVER_URL/pypi/simple/
EOF
    if [ $? -eq 0 ]; then
        echo "✅ pip configured successfully"
    else
        echo "❌ Failed to configure pip"
    fi
}}

# Configure npm
configure_npm() {{
    echo "📦 Configuring npm..."
    npm config set registry $SERVER_URL/npm/ 2>/dev/null
    if [ $? -eq 0 ]; then
        echo "✅ npm configured successfully"
    else
        echo "❌ Failed to configure npm (npm might not be installed)"
    fi
}}

# Configure cargo
configure_cargo() {{
    echo "📦 Configuring cargo..."
    mkdir -p ~/.cargo
    cat > ~/.cargo/config.toml << EOF
[source.crates-io]
replace-with = "goobits"

[source.goobits]
registry = "$SERVER_URL/cargo/"
EOF
    if [ $? -eq 0 ]; then
        echo "✅ cargo configured successfully"
    else
        echo "❌ Failed to configure cargo"
    fi
}}

# Run configurations
configure_pip
configure_npm
configure_cargo

echo ""
echo "✨ Configuration complete! Package managers will now check your local server first."
echo ""
echo "To test:"
echo "  pip install <package>    # Will check $SERVER_URL first"
echo "  npm install <package>    # Will check $SERVER_URL first"
echo "  cargo install <package>  # Will check $SERVER_URL first"
echo ""
echo "To restore original settings, run:"
echo "  rm ~/.pip/pip.conf"
echo "  npm config delete registry"
echo "  rm ~/.cargo/config.toml"
"#,
        safe_server_ip = safe_server_ip,
        safe_port = safe_port
    )
}

/// Serve the setup script via HTTP endpoint
async fn serve_setup_script(headers: axum::http::HeaderMap) -> impl IntoResponse {
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("localhost:3080")
        .to_string();
    // Extract IP and port from the Host header
    let (server_ip, port) = if let Some((ip, port_str)) = host.rsplit_once(':') {
        let port = port_str.parse::<u16>().unwrap_or(3080);
        (ip.to_string(), port)
    } else {
        (host, 3080)
    };

    // Validate inputs to prevent injection
    if !validate_server_address(&server_ip) {
        return (
            StatusCode::BAD_REQUEST,
            [
                ("content-type", "text/plain"),
                ("content-disposition", "inline"),
            ],
            "Invalid server address".to_string(),
        );
    }

    let script = create_setup_script_content(&server_ip, port);

    (
        StatusCode::OK,
        [
            ("content-type", "text/x-shellscript"),
            ("content-disposition", "inline; filename=\"setup.sh\""),
        ],
        script,
    )
}
