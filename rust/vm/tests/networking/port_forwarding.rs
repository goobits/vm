use anyhow::{Context, Result};
use assert_cmd::prelude::*;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::{tempdir, TempDir};
use uuid::Uuid;

/// Check if Docker daemon is available
fn is_docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Skip test if Docker is not available
macro_rules! require_docker {
    () => {
        if !is_docker_available() {
            eprintln!("⚠️  Skipping test: Docker daemon not available");
            eprintln!("   To run this test, ensure Docker is running");
            return Ok(());
        }
    };
}

/// A test fixture that creates a temporary project directory and ensures
/// the VM is destroyed when the fixture goes out of scope.
struct TestFixture {
    path: PathBuf,
    _tempdir: TempDir, // Held to ensure the directory is cleaned up after the test
}

impl TestFixture {
    /// Creates a new test fixture with a temporary directory.
    fn new() -> Result<Self> {
        let tempdir = tempdir()?;
        let path = tempdir.path().to_path_buf();
        Ok(Self {
            path,
            _tempdir: tempdir,
        })
    }

    /// Returns the path to the temporary project directory.
    fn path(&self) -> &Path {
        &self.path
    }

    /// Runs a `vm` command within the temporary project directory and asserts success.
    /// This should be used for the main test logic.
    fn run_vm_command(&self, args: &[&str]) -> Result<()> {
        let mut cmd = Command::cargo_bin("vm")?;
        cmd.current_dir(self.path());
        cmd.args(args);
        cmd.assert().success();
        Ok(())
    }

    /// Runs a `vm` command that is expected to fail and returns the assertion object.
    fn run_failing_vm_command(&self, args: &[&str]) -> Result<assert_cmd::assert::Assert> {
        let mut cmd = Command::cargo_bin("vm")?;
        cmd.current_dir(self.path());
        cmd.args(args);
        Ok(cmd.assert().failure())
    }

    /// Runs the cleanup command without asserting its success. This is critical
    /// for the Drop implementation to prevent panics during cleanup.
    fn run_cleanup_command(&self) {
        // Use `vm destroy` to clean up - it handles all Docker cleanup without sudo
        if let Ok(mut cmd) = Command::cargo_bin("vm") {
            cmd.current_dir(self.path());
            cmd.args(["destroy", "--force"]);
            // Do not assert success. Just run it and ignore the result.
            // This prevents a panic in a panic, which would abort the test runner.
            let _ = cmd.output();
        }
    }
}

/// The Drop implementation ensures that `vm destroy` is called for cleanup,
/// even if the test panics. It uses a non-panicking command runner.
impl Drop for TestFixture {
    fn drop(&mut self) {
        self.run_cleanup_command();
    }
}

#[test]
fn test_port_forwarding_single_port() -> Result<()> {
    require_docker!();

    let project_name = format!("test-port-forwarding-{}", Uuid::new_v4());
    let fixture = TestFixture::new()?;
    let vm_yaml_path = fixture.path().join("vm.yaml");

    let config = format!(
        r#"
version: 1
provider: docker
project:
  name: {}
vm:
  cpus: 1
  memory: 1024
ports:
  mappings:
    - host: 3456
      guest: 3000
"#,
        project_name
    );
    fs::write(&vm_yaml_path, config)?;

    // Create and start VM
    fixture.run_vm_command(&["create"])?;

    // Verify port mapping exists
    let container_name = format!("{}-dev", project_name);
    let output = Command::new("docker")
        .args(["port", &container_name, "3000"])
        .output()?;

    let port_info = String::from_utf8(output.stdout)?;
    assert!(
        port_info.contains("0.0.0.0:3456"),
        "Port 3456 should be mapped to guest 3000. Got: {}",
        port_info
    );

    Ok(())
}

#[test]
fn test_port_forwarding_multiple_ports() -> Result<()> {
    require_docker!();

    let project_name = format!("test-port-forwarding-{}", Uuid::new_v4());
    let fixture = TestFixture::new()?;
    let vm_yaml_path = fixture.path().join("vm.yaml");

    let config = format!(
        r#"
version: 1
provider: docker
project:
  name: {}
vm:
  cpus: 1
  memory: 1024
ports:
  mappings:
    - host: 3457
      guest: 3001
    - host: 3458
      guest: 3002
      protocol: udp
"#,
        project_name
    );
    fs::write(&vm_yaml_path, config)?;

    // Create and start VM
    fixture.run_vm_command(&["create"])?;

    let container_name = format!("{}-dev", project_name);

    // Verify first port mapping
    let output1 = Command::new("docker")
        .args(["port", &container_name, "3001"])
        .output()?;
    let port_info1 = String::from_utf8(output1.stdout)?;
    assert!(
        port_info1.contains("0.0.0.0:3457"),
        "Port 3457 should be mapped to guest 3001. Got: {}",
        port_info1
    );

    // Verify second port mapping (UDP)
    let output2 = Command::new("docker")
        .args(["port", &container_name, "3002/udp"])
        .output()?;
    let port_info2 = String::from_utf8(output2.stdout)?;
    assert!(
        port_info2.contains("0.0.0.0:3458"),
        "Port 3458/udp should be mapped to guest 3002. Got: {}",
        port_info2
    );

    Ok(())
}

#[test]
fn test_port_conflict_detection() -> Result<()> {
    let project_name = format!("test-port-forwarding-{}", Uuid::new_v4());
    let fixture = TestFixture::new()?;
    let vm_yaml_path = fixture.path().join("vm.yaml");

    // Start a temporary server on port 3333 to create a conflict
    let _listener =
        TcpListener::bind("0.0.0.0:3333").context("Failed to bind to port 3333 for testing")?;

    // Try to create VM with conflicting port
    let config = format!(
        r#"
version: 1
provider: docker
project:
  name: {}
vm:
  cpus: 1
  memory: 1024
ports:
  mappings:
    - host: 3333
      guest: 3000
"#,
        project_name
    );
    fs::write(&vm_yaml_path, config)?;

    // `vm create` should fail with a clear error on stdout
    fixture
        .run_failing_vm_command(&["create"])?
        .stdout(predicates::str::contains(
            "Configuration error: Port 3333 is already in use on host",
        ));

    Ok(())
}
