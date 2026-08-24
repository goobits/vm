//! Common test utilities and helpers
//!
//! This module provides shared functionality for all test files to reduce code duplication
//! and improve maintainability of the test suite.

#![allow(dead_code)]

use anyhow::{anyhow, bail, Result};
use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;
use vm_package_server::{AppState, Config, ResolverService, UpstreamClient, UpstreamConfig};

/// Test server configuration
pub struct TestServerConfig {
    pub port: u16,
    pub data_dir: std::path::PathBuf,
}

/// Test server setup result
pub struct TestSetup {
    pub temp_dir: TempDir,
    pub app_state: Arc<AppState>,
    pub config: TestServerConfig,
}

/// Creates a test server setup with temporary directories and app state
///
/// This function handles the common setup required for most integration tests:
/// - Creates temporary directories for each package ecosystem
/// - Sets up upstream client configuration
/// - Returns app state that can be used for testing
pub async fn create_test_setup() -> Result<TestSetup> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().to_path_buf();

    // Create required directories for all package ecosystems
    create_package_directories(&data_dir)?;

    let upstream_config = UpstreamConfig {
        enabled: false,
        ..Default::default()
    };
    let upstream_client = Arc::new(UpstreamClient::new(upstream_config).unwrap());
    let config = Arc::new(Config::default());

    let app_state = Arc::new(AppState {
        data_dir: data_dir.clone(),
        server_addr: "http://localhost:8080".to_string(),
        upstream_client,
        internal_client: None,
        config,
        resolver: Arc::new(ResolverService::standalone()),
    });

    // Find available port for testing
    let port = find_available_port()?;

    let config = TestServerConfig { port, data_dir };

    Ok(TestSetup {
        temp_dir,
        app_state,
        config,
    })
}

/// Creates all required package ecosystem directories
pub fn create_package_directories(data_dir: &Path) -> Result<()> {
    // PyPI directories
    fs::create_dir_all(data_dir.join("pypi/packages"))?;

    // NPM directories
    fs::create_dir_all(data_dir.join("npm/tarballs"))?;
    fs::create_dir_all(data_dir.join("npm/metadata"))?;

    // Cargo directories
    fs::create_dir_all(data_dir.join("cargo/crates"))?;
    fs::create_dir_all(data_dir.join("cargo/index"))?;

    Ok(())
}

/// Finds an available port for testing
pub fn find_available_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Starts a test server process and waits for it to be ready
///
/// Returns the ready process. The caller is responsible for killing it.
pub async fn start_test_server(
    port: u16,
    data_dir: &Path,
    additional_args: &[&str],
) -> Result<std::process::Child> {
    let port_str = port.to_string();
    let data_str = data_dir.to_str().unwrap();

    let mut args = vec!["start", "--port", &port_str, "--data", data_str];
    args.extend(additional_args);

    let executable = option_env!("CARGO_BIN_EXE_pkg-server")
        .ok_or_else(|| anyhow!("pkg-server test binary is unavailable"))?;
    let mut child = Command::new(executable)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for server to start with retry logic
    if wait_for_server_start(port).await {
        return Ok(child);
    }

    child.kill().ok();
    let output = child.wait_with_output()?;
    bail!(
        "test server failed to start\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Waits for a server to start on the given port
pub async fn wait_for_server_start(port: u16) -> bool {
    const MAX_ATTEMPTS: u32 = 30;
    const RETRY_DELAY: Duration = Duration::from_millis(500);
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

    for _ in 0..MAX_ATTEMPTS {
        tokio::time::sleep(RETRY_DELAY).await;

        let client = reqwest::Client::new();
        if let Ok(Ok(resp)) = timeout(
            REQUEST_TIMEOUT,
            client
                .get(format!("http://localhost:{}/api/status", port))
                .send(),
        )
        .await
        {
            if resp.status().is_success() {
                return true;
            }
        }
    }

    false
}

/// Kills a server process and captures its output for debugging
pub fn kill_server_with_output(mut child: std::process::Child) -> Result<()> {
    child.kill().ok();
    if let Ok(output) = child.wait_with_output() {
        if !output.status.success() {
            eprintln!("Server stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("Server stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
    Ok(())
}
