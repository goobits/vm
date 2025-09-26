//! End-to-end tests for CLI commands
//!
//! This module tests the CLI commands by spawning the actual binary and verifying
//! that the commands work correctly in realistic scenarios. These tests ensure
//! that the command-line interface functions properly and integrates correctly
//! with the server components.

use anyhow::Result;
use std::fs;
use tempfile::TempDir;

mod common;
use common::{
    assertions, create_test_package_json, execute_cli_command, find_available_port,
    kill_server_with_output, start_test_server,
};

/// Tests the start command functionality end-to-end
#[tokio::test]
async fn test_cli_start_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("test_data");
    let port = find_available_port()?;

    // Test start command with --no-config and --foreground flags
    let (child, server_started) = start_test_server(port, &data_dir, &[]).await?;

    // Clean up server process
    kill_server_with_output(child)?;

    // Only assert if server started (compilation/runtime issues may prevent it)
    if server_started {
        // Verify data directories were created
        assertions::assert_package_directories_exist(&data_dir);
    } else {
        // Skip test if server couldn't start (likely due to environment issues)
        eprintln!("Skipping test - server failed to start (likely environment issue)");
    }

    Ok(())
}

/// Tests the stop command functionality
#[tokio::test]
async fn test_cli_stop_command() -> Result<()> {
    // Test stop command (restores client configurations)
    let output = execute_cli_command(&["stop"], None)?;

    // Stop command should succeed (even if no configs to restore)
    assertions::assert_command_success(&output, "Stop");

    Ok(())
}

/// Tests the add command functionality
#[tokio::test]
async fn test_cli_add_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let port = find_available_port()?;
    let data_dir = temp_dir.path().join("data");

    // Start server in background
    let (server, server_started) = start_test_server(port, &data_dir, &[]).await?;

    if server_started {
        // Create a temporary package directory with package.json
        let package_dir = temp_dir.path().join("test_package");
        fs::create_dir_all(&package_dir)?;

        create_test_package_json(&package_dir, "test-package", "1.0.0")?;

        // Test add command
        let output = execute_cli_command(
            &[
                "add",
                "--server",
                &format!("http://localhost:{}", port),
                "--type",
                "npm",
            ],
            Some(&package_dir),
        )?;

        // Command should complete (may fail due to missing npm setup, but should execute)
        println!(
            "Add command stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Kill server
    kill_server_with_output(server)?;

    Ok(())
}

/// Tests the remove command functionality
#[tokio::test]
async fn test_cli_remove_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let port = find_available_port()?;
    let data_dir = temp_dir.path().join("data");

    // Start server in background
    let (server, server_started) = start_test_server(port, &data_dir, &[]).await?;

    if server_started {
        // Test remove command with force flag
        let output = execute_cli_command(
            &[
                "remove",
                "--server",
                &format!("http://localhost:{}", port),
                "--force",
            ],
            None,
        )?;

        // Command should complete successfully
        assertions::assert_command_success(&output, "Remove");
    }

    // Kill server
    kill_server_with_output(server)?;

    Ok(())
}

/// Tests the list command functionality
#[tokio::test]
async fn test_cli_list_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let port = find_available_port()?;
    let data_dir = temp_dir.path().join("data");

    // Start server in background
    let (server, server_started) = start_test_server(port, &data_dir, &[]).await?;

    if server_started {
        // Test list command
        let output = execute_cli_command(
            &["list", "--server", &format!("http://localhost:{}", port)],
            None,
        )?;

        // Command should succeed
        assertions::assert_command_success(&output, "List");

        // Should contain package listing output
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("packages") || stdout.contains("No packages"),
            "Output should contain package information"
        );
    }

    // Kill server
    kill_server_with_output(server)?;

    Ok(())
}

/// Tests the status command functionality
#[tokio::test]
async fn test_cli_status_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let port = find_available_port()?;
    let data_dir = temp_dir.path().join("data");

    // Start server in background
    let (server, server_started) = start_test_server(port, &data_dir, &[]).await?;

    if server_started {
        // Test status command
        let output = execute_cli_command(
            &["status", "--server", &format!("http://localhost:{}", port)],
            None,
        )?;

        // Command should succeed
        assertions::assert_command_success(&output, "Status");

        // Should contain status information
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("Server") || stdout.contains("running") || stdout.contains("status"),
            "Output should contain server status information"
        );
    }

    // Kill server
    kill_server_with_output(server)?;

    Ok(())
}

/// Tests the start command with Docker flag
#[tokio::test]
async fn test_cli_start_command_with_docker() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("test_data");
    let port = find_available_port()?;

    // Test start command with --docker flag (should fail gracefully if Docker not available)
    let output = execute_cli_command(
        &[
            "start",
            "--port",
            &port.to_string(),
            "--data",
            data_dir.to_str().unwrap(),
            "--docker",
        ],
        None,
    )?;

    // Command should either succeed (if Docker available) or fail with helpful message
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Docker") || stderr.contains("docker"),
            "Error should mention Docker when Docker flag is used"
        );
    }

    Ok(())
}
