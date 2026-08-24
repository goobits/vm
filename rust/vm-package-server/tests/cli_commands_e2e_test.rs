//! End-to-end tests for CLI commands
//!
//! This module tests the CLI commands by spawning the actual binary and verifying
//! that the commands work correctly in realistic scenarios. These tests ensure
//! that the command-line interface functions properly and integrates correctly
//! with the server components.

use anyhow::{anyhow, bail, Result};
use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

fn find_available_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

async fn start_test_server(port: u16, data_dir: &Path) -> Result<std::process::Child> {
    let executable = option_env!("CARGO_BIN_EXE_pkg-server")
        .ok_or_else(|| anyhow!("pkg-server test binary is unavailable"))?;
    let port = port.to_string();
    let mut child = Command::new(executable)
        .args([
            "start",
            "--port",
            &port,
            "--data",
            data_dir.to_str().expect("temporary path should be UTF-8"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let request = reqwest::get(format!("http://localhost:{port}/api/status"));
        if matches!(timeout(Duration::from_secs(3), request).await, Ok(Ok(response)) if response.status().is_success())
        {
            return Ok(child);
        }
    }

    child.kill().ok();
    let output = child.wait_with_output()?;
    bail!(
        "test server failed to start\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn kill_server_with_output(mut child: std::process::Child) {
    child.kill().ok();
    if let Ok(output) = child.wait_with_output() {
        if !output.status.success() {
            eprintln!("Server stdout: {}", String::from_utf8_lossy(&output.stdout));
            eprintln!("Server stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    }
}

/// Tests the start command functionality end-to-end
#[tokio::test]
async fn test_cli_start_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("test_data");
    let port = find_available_port()?;

    let child = start_test_server(port, &data_dir).await?;
    kill_server_with_output(child);

    Ok(())
}
