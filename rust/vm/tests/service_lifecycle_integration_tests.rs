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

fn is_docker_running() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .is_ok_and(|o| o.status.success())
}

#[test]
fn test_shared_postgres_lifecycle_integration() -> Result<()> {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running or not available.");
        return Ok(());
    }

    let _guard = TEST_MUTEX.lock().unwrap();
    let temp_dir = TempDir::new()?;
    let home_dir = temp_dir.path();
    let project_dir = home_dir.join("test-project");
    std::fs::create_dir_all(&project_dir)?;

    let vm_binary = assert_cmd::cargo::cargo_bin("vm");

    // 1. Enable shared postgresql globally
    let output = Command::new(&vm_binary)
        .args(["config", "set", "services.postgresql.enabled", "true"])
        .env("HOME", home_dir)
        .output()?;
    assert!(
        output.status.success(),
        "Failed to enable postgresql service. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Create a VM. This should start the postgres service.
    let output = Command::new(&vm_binary)
        .args(["create"])
        .current_dir(&project_dir)
        .env("HOME", home_dir)
        .env("VM_NO_PROMPT", "true") // for `vm init`
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "vm create failed. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
    assert!(stdout.contains("Starting PostgreSQL"));

    // 3. Verify the postgres container is running
    let ps_output = Command::new("docker")
        .args(["ps", "--filter", "name=vm-postgres-global"])
        .output()?;
    let ps_stdout = String::from_utf8_lossy(&ps_output.stdout);
    assert!(
        ps_stdout.contains("vm-postgres-global"),
        "Postgres container not found after vm create. Output: {}",
        ps_stdout
    );

    // 4. Destroy the VM. This should stop the postgres service.
    let output = Command::new(&vm_binary)
        .args(["destroy", "--yes"])
        .current_dir(&project_dir)
        .env("HOME", home_dir)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "vm destroy failed. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
    assert!(stdout.contains("Stopping PostgreSQL"));

    // 5. Verify the postgres container is stopped and removed.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let ps_output_after = Command::new("docker")
        .args(["ps", "-a", "--filter", "name=vm-postgres-global"])
        .output()?;
    let ps_stdout_after = String::from_utf8_lossy(&ps_output_after.stdout);
    assert!(
        !ps_stdout_after.contains("vm-postgres-global"),
        "Postgres container was not removed after vm destroy. Output: {}",
        ps_stdout_after
    );

    Ok(())
}

#[test]
fn test_shared_redis_lifecycle_integration() -> Result<()> {
    if !is_docker_running() {
        println!("Skipping test: Docker is not running or not available.");
        return Ok(());
    }

    let _guard = TEST_MUTEX.lock().unwrap();
    let temp_dir = TempDir::new()?;
    let home_dir = temp_dir.path();
    let project_dir = home_dir.join("test-project-redis");
    std::fs::create_dir_all(&project_dir)?;

    let vm_binary = assert_cmd::cargo::cargo_bin("vm");

    // 1. Enable shared redis globally
    let output = Command::new(&vm_binary)
        .args(["config", "set", "services.redis.enabled", "true"])
        .env("HOME", home_dir)
        .output()?;
    assert!(
        output.status.success(),
        "Failed to enable redis service. Stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // 2. Create a VM. This should start the redis service.
    let output = Command::new(&vm_binary)
        .args(["create"])
        .current_dir(&project_dir)
        .env("HOME", home_dir)
        .env("VM_NO_PROMPT", "true")
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "vm create failed for redis. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
    assert!(stdout.contains("Starting Redis"));

    // 3. Verify the redis container is running
    let ps_output = Command::new("docker")
        .args(["ps", "--filter", "name=vm-redis-global"])
        .output()?;
    let ps_stdout = String::from_utf8_lossy(&ps_output.stdout);
    assert!(
        ps_stdout.contains("vm-redis-global"),
        "Redis container not found. Output: {}",
        ps_stdout
    );

    // 4. Destroy the VM. This should stop the redis service.
    let output = Command::new(&vm_binary)
        .args(["destroy", "--yes"])
        .current_dir(&project_dir)
        .env("HOME", home_dir)
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Stopping Redis"));

    // 5. Verify the redis container is stopped and removed.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let ps_output_after = Command::new("docker")
        .args(["ps", "-a", "--filter", "name=vm-redis-global"])
        .output()?;
    let ps_stdout_after = String::from_utf8_lossy(&ps_output_after.stdout);
    assert!(
        !ps_stdout_after.contains("vm-redis-global"),
        "Redis container not removed. Output: {}",
        ps_stdout_after
    );

    Ok(())
}
