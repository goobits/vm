//! Service lifecycle management integration tests
//!
//! These tests verify that the service lifecycle management works correctly
//! at the CLI level, testing the actual user-facing behavior.

use anyhow::Result;
use assert_cmd::prelude::*; // For CommandCargoExt
use std::process::Command;
use std::sync::Mutex;
use tempfile::TempDir;

// Test synchronization to prevent race conditions
static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Test fixture for CLI integration tests
struct CliTestFixture {
    _temp_dir: TempDir,
    vm_binary: std::path::PathBuf,
}

impl CliTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;

        // Get the path to the vm binary
        let vm_binary = Command::cargo_bin("vm")?.get_program().into();

        Ok(Self {
            _temp_dir: temp_dir,
            vm_binary,
        })
    }

    /// Run a vm command and return the output
    fn run_vm_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(&self.vm_binary)
            .args(args)
            .current_dir(self._temp_dir.path())
            .output()?;

        Ok(output)
    }
}

#[test]
fn test_vm_auth_help_excludes_start_stop() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;
    let output = fixture.run_vm_command(&["auth", "--help"])?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the help doesn't include start/stop commands
    assert!(!stdout.contains("Start auth proxy"));
    assert!(!stdout.contains("Stop auth proxy"));

    // Verify it still includes the expected commands
    assert!(stdout.contains("status"));
    assert!(stdout.contains("add"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("remove"));
    assert!(stdout.contains("interactive"));

    Ok(())
}

#[test]
fn test_vm_registry_help_excludes_start_stop() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;

    // Test that registry command no longer exists (auto-managed design)
    let output = fixture.run_vm_command(&["registry", "--help"]);

    // The registry command should no longer exist at all
    assert!(output.is_err() || !output.unwrap().status.success());

    // Verify that the main help doesn't include registry commands
    let help_output = fixture.run_vm_command(&["--help"])?;
    let help_stdout = String::from_utf8_lossy(&help_output.stdout);

    // Should not mention manual registry management
    assert!(!help_stdout.contains("Docker registry management"));

    Ok(())
}

#[test]
fn test_vm_pkg_help_excludes_start_stop() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;
    let output = fixture.run_vm_command(&["pkg", "--help"])?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify the help doesn't include start/stop commands
    assert!(!stdout.contains("Start package registry"));
    assert!(!stdout.contains("Stop package registry"));

    // Verify it still includes the expected commands
    assert!(stdout.contains("status"));
    assert!(stdout.contains("add"));
    assert!(stdout.contains("list"));
    assert!(stdout.contains("remove"));
    assert!(stdout.contains("config"));
    assert!(stdout.contains("use"));

    Ok(())
}

#[test]
fn test_vm_auth_status_shows_lifecycle_info() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;
    let output = fixture.run_vm_command(&["auth", "status"])?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify status includes lifecycle management information
    assert!(stdout.contains("automatically managed"));
    assert!(stdout.contains("VM lifecycle"));

    Ok(())
}

#[test]
fn test_vm_registry_status_shows_lifecycle_info() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;

    // Test that registry status command no longer exists (auto-managed design)
    let output = fixture.run_vm_command(&["registry", "status"]);

    // The registry command should no longer exist at all
    assert!(output.is_err() || !output.unwrap().status.success());

    // Registry is now managed automatically by service manager
    // Status can be checked via doctor command instead

    Ok(())
}

#[test]
fn test_vm_pkg_status_shows_lifecycle_info() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;
    // Use --yes flag to auto-start server without prompting
    let output = fixture.run_vm_command(&["pkg", "status", "--yes"])?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify status includes lifecycle management information
    assert!(stdout.contains("automatically managed"));
    assert!(stdout.contains("VM lifecycle"));

    Ok(())
}

#[test]
fn test_auth_start_command_no_longer_exists() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;
    let output = fixture.run_vm_command(&["auth", "start"])?;

    // This should fail since start command no longer exists
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("subcommand"));

    Ok(())
}

#[test]
fn test_registry_start_command_no_longer_exists() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;
    let output = fixture.run_vm_command(&["registry", "start"])?;

    // This should fail since start command no longer exists
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("subcommand"));

    Ok(())
}

#[test]
fn test_pkg_start_command_no_longer_exists() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    let fixture = CliTestFixture::new()?;
    let output = fixture.run_vm_command(&["pkg", "start"])?;

    // This should fail since start command no longer exists
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unexpected argument") || stderr.contains("subcommand"));

    Ok(())
}

// Integration test that would test actual VM creation with service management
#[test]
fn test_vm_lifecycle_integration() -> Result<()> {
    // Only run if integration tests are explicitly requested
    if std::env::var("VM_INTEGRATION_TESTS").is_err() {
        println!("Skipping integration test - set VM_INTEGRATION_TESTS=1 to run");
        return Ok(());
    }

    let _guard = TEST_MUTEX.lock().unwrap();

    // This would test:
    // 1. Create VM with services enabled in config
    // 2. Verify services auto-start
    // 3. Destroy VM
    // 4. Verify services auto-stop when no VMs using them

    Ok(())
}
