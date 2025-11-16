//! Common test utilities and helpers
//!
//! This module provides shared functionality for all test files to reduce code duplication
//! and improve maintainability of the test suite.

#![allow(dead_code)]

use anyhow::Result;
use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;
use vm_package_server::config::Config;
use vm_package_server::{upstream::UpstreamConfig, AppState, UpstreamClient};

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
        config,
        npm_registry: vm_package_server::registry::NpmRegistry::new(),
        pypi_registry: vm_package_server::registry::PypiRegistry::new(),
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
/// Returns the spawned process and whether the server started successfully.
/// The caller is responsible for killing the process.
pub async fn start_test_server(
    port: u16,
    data_dir: &Path,
    additional_args: &[&str],
) -> Result<(std::process::Child, bool)> {
    let port_str = port.to_string();
    let data_str = data_dir.to_str().unwrap();

    let mut args = vec![
        "run",
        "--bin",
        "pkg-server",
        "--",
        "start",
        "--port",
        &port_str,
        "--data",
        data_str,
        "--no-config",
        "--foreground",
    ];
    args.extend(additional_args);

    let child = Command::new("cargo")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // Wait for server to start with retry logic
    let server_started = wait_for_server_start(port).await;

    Ok((child, server_started))
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

/// Executes a CLI command and returns the output
pub fn execute_cli_command(
    args: &[&str],
    current_dir: Option<&Path>,
) -> Result<std::process::Output> {
    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "--features=standalone-binary",
        "--bin",
        "pkg-server",
        "--",
    ]);
    cmd.args(args);

    if let Some(dir) = current_dir {
        cmd.current_dir(dir);
    }

    Ok(cmd.output()?)
}

/// Creates a test package.json file for NPM tests
pub fn create_test_package_json(dir: &Path, name: &str, version: &str) -> Result<()> {
    let package_json = format!(
        r#"{{
    "name": "{}",
    "version": "{}",
    "description": "Test package for E2E testing"
}}"#,
        name, version
    );
    fs::write(dir.join("package.json"), package_json)?;
    Ok(())
}

/// Common assertion helpers
pub mod assertions {
    use std::path::Path;

    /// Asserts that all standard package directories exist
    pub fn assert_package_directories_exist(data_dir: &Path) {
        assert!(
            data_dir.join("pypi/packages").exists(),
            "PyPI directory should be created"
        );
        assert!(
            data_dir.join("npm/tarballs").exists(),
            "NPM directory should be created"
        );
        assert!(
            data_dir.join("cargo/crates").exists(),
            "Cargo directory should be created"
        );
    }

    /// Asserts that a command completed successfully with helpful error message
    pub fn assert_command_success(output: &std::process::Output, command_name: &str) {
        if !output.status.success() {
            eprintln!("{} command failed:", command_name);
            eprintln!("stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
        assert!(
            output.status.success(),
            "{} command should succeed",
            command_name
        );
    }

    /// Asserts that command output contains expected content
    pub fn assert_output_contains(output: &std::process::Output, expected: &str, context: &str) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(expected),
            "{} output should contain '{}', got: {}",
            context,
            expected,
            stdout
        );
    }
}

/// Test data management utilities
pub mod test_data {
    use anyhow::Result;
    use std::fs;
    use std::path::Path;

    /// Creates a minimal Cargo.toml file for testing
    pub fn create_test_cargo_toml(dir: &Path, name: &str, version: &str) -> Result<()> {
        let cargo_toml = format!(
            r#"[package]
name = "{}"
version = "{}"
edition = "2021"

[dependencies]
"#,
            name, version
        );
        fs::write(dir.join("Cargo.toml"), cargo_toml)?;

        // Create src/lib.rs
        fs::create_dir_all(dir.join("src"))?;
        fs::write(dir.join("src/lib.rs"), "// Test library\n")?;

        Ok(())
    }

    /// Creates a minimal setup.py file for testing
    pub fn create_test_setup_py(dir: &Path, name: &str, version: &str) -> Result<()> {
        let setup_py = format!(
            r#"from setuptools import setup

setup(
    name="{}",
    version="{}",
    description="Test package",
    py_modules=["{}"],
)
"#,
            name, version, name
        );
        fs::write(dir.join("setup.py"), setup_py)?;

        // Create a simple Python module
        fs::write(dir.join(format!("{}.py", name)), "# Test module\n")?;

        Ok(())
    }

    /// Cleanup utility for removing generated files
    pub fn cleanup_build_artifacts(dir: &Path) -> Result<()> {
        // Remove common build artifacts
        let artifacts = ["target", "dist", "build", "*.egg-info"];

        for artifact in artifacts {
            let path = dir.join(artifact);
            if path.exists() {
                if path.is_dir() {
                    fs::remove_dir_all(&path)?;
                } else {
                    fs::remove_file(&path)?;
                }
            }
        }

        // Remove .tgz files from NPM pack
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if let Some(ext) = entry.path().extension() {
                if ext == "tgz" {
                    fs::remove_file(entry.path())?;
                }
            }
        }

        Ok(())
    }
}

/// Compatibility layer for axum-test version issues
pub mod compat {
    use axum::Router;
    use std::sync::Arc;
    use vm_package_server::AppState;

    /// Creates a compatible test service from a router
    ///
    /// This function handles the axum version compatibility issues
    /// by converting the router to an IntoMakeService which is compatible with axum-test
    pub fn make_test_service(router: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
        router
    }
}
