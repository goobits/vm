//! End-to-end tests for CLI commands
//!
//! This module tests the CLI commands by spawning the actual binary and verifying
//! that the commands work correctly in realistic scenarios. These tests ensure
//! that the command-line interface functions properly and integrates correctly
//! with the server components.

use anyhow::Result;
use tempfile::TempDir;

mod common;
use common::{find_available_port, kill_server_with_output, start_test_server};

/// Tests the start command functionality end-to-end
#[tokio::test]
async fn test_cli_start_command() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let data_dir = temp_dir.path().join("test_data");
    let port = find_available_port()?;

    let child = start_test_server(port, &data_dir, &[]).await?;
    kill_server_with_output(child)?;

    Ok(())
}
