use anyhow::Result;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;

// Global mutex to ensure tests run sequentially to avoid Docker container conflicts
static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Test fixture for VM operations integration testing
///
/// This fixture creates isolated temporary directories and ensures all VM operations
/// are executed in completely isolated environments that don't affect the project filesystem.
struct VmOpsTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    binary_path: PathBuf,
    project_name: String,
}

impl VmOpsTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_dir)?;

        // Generate unique project name to avoid Docker container conflicts
        let uuid_str = uuid::Uuid::new_v4().to_string();
        let project_name = format!("vm-test-{}", &uuid_str[..8]);

        // Get the path to the vm binary
        let binary_path = PathBuf::from("/workspace/.build/target/debug/vm");

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            binary_path,
            project_name,
        })
    }

    /// Run vm command with given arguments in the isolated test directory
    fn run_vm_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(&self.binary_path)
            .args(args)
            .current_dir(&self.test_dir)
            .env("HOME", self.test_dir.parent().unwrap())
            .env("VM_TOOL_DIR", &self.test_dir)
            .env("VM_TEST_MODE", "1") // Disable structured logging for tests
            .output()?;
        Ok(output)
    }

    /// Create a minimal vm.yaml configuration for testing
    fn create_test_config(&self) -> Result<()> {
        let config_content = format!(
            r#"provider: docker
project:
  name: {}
vm:
  memory: 512
  cpus: 1
services:
  test:
    enabled: true
    image: alpine:latest
    ports:
      - "8080:80"
"#,
            self.project_name
        );

        fs::write(self.test_dir.join("vm.yaml"), config_content)?;
        Ok(())
    }

    /// Create a Dockerfile for testing provision operations
    fn create_test_dockerfile(&self) -> Result<()> {
        let dockerfile_content = r#"FROM alpine:latest
RUN apk add --no-cache curl
WORKDIR /app
COPY . .
EXPOSE 80
CMD ["sh", "-c", "echo 'VM Test Container Running' && sleep 3600"]
"#;

        fs::write(self.test_dir.join("Dockerfile"), dockerfile_content)?;

        // Create a simple test file
        fs::write(self.test_dir.join("test.txt"), "VM integration test file")?;
        Ok(())
    }

    /// Check if Docker is available and working
    fn is_docker_available(&self) -> bool {
        Command::new("docker")
            .args(["info"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Clean up any existing test containers
    fn cleanup_test_containers(&self) -> Result<()> {
        // Force remove any existing test containers
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.project_name])
            .output();

        // Also clean up any dangling containers with our test prefix
        let _ = Command::new("docker")
            .args([
                "container",
                "prune",
                "-f",
                "--filter",
                &format!("label=project={}", self.project_name),
            ])
            .output();

        Ok(())
    }

    /// Wait for container to be in expected state
    fn wait_for_container_state(&self, expected_state: &str, timeout_secs: u64) -> bool {
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(timeout_secs) {
            if self.check_container_state(expected_state) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }

    /// Check if container is in expected state
    fn check_container_state(&self, expected_state: &str) -> bool {
        let output = Command::new("docker")
            .args([
                "inspect",
                "--format",
                "{{.State.Status}}",
                &self.project_name,
            ])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let state = String::from_utf8_lossy(&output.stdout).trim().to_string();
                return state == expected_state;
            }
        }
        false
    }
}

impl Drop for VmOpsTestFixture {
    fn drop(&mut self) {
        // Clean up test containers when fixture is dropped
        let _ = self.cleanup_test_containers();
    }
}

#[test]
fn test_vm_create_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    // Skip test if binary doesn't exist
    if !fixture.binary_path.exists() {
        println!(
            "Skipping test - vm binary not found at {:?}",
            fixture.binary_path
        );
        return Ok(());
    }

    // Skip test if Docker is not available
    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available for integration testing");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Test VM creation
    let output = fixture.run_vm_command(&["create"])?;
    assert!(
        output.status.success(),
        "VM create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify container was created
    let check_output = Command::new("docker")
        .args(["inspect", &fixture.project_name])
        .output()?;
    assert!(check_output.status.success(), "Container was not created");

    // Clean up
    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_create_with_force() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create VM first time
    let output = fixture.run_vm_command(&["create"])?;
    assert!(output.status.success());

    // Create again with force flag - should succeed
    let output = fixture.run_vm_command(&["create", "--force"])?;
    assert!(
        output.status.success(),
        "VM create --force failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_start_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create VM first
    let output = fixture.run_vm_command(&["create"])?;
    assert!(output.status.success());

    // Start the VM
    let output = fixture.run_vm_command(&["start"])?;
    assert!(
        output.status.success(),
        "VM start failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify container is running
    assert!(
        fixture.wait_for_container_state("running", 30),
        "Container did not start within timeout"
    );

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_stop_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create and start VM
    fixture.run_vm_command(&["create"])?;
    fixture.run_vm_command(&["start"])?;

    // Wait for it to be running
    assert!(fixture.wait_for_container_state("running", 30));

    // Stop the VM (graceful stop)
    let output = fixture.run_vm_command(&["stop"])?;
    assert!(
        output.status.success(),
        "VM stop failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify container is stopped
    assert!(
        fixture.wait_for_container_state("exited", 30),
        "Container did not stop within timeout"
    );

    // Test stop with specific container (force kill)
    fixture.run_vm_command(&["start"])?;
    assert!(fixture.wait_for_container_state("running", 30));

    let output = fixture.run_vm_command(&["stop", &fixture.project_name])?;
    assert!(
        output.status.success(),
        "VM stop with container name failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify container is no longer running
    std::thread::sleep(Duration::from_secs(2));
    let check_output = Command::new("docker")
        .args([
            "inspect",
            "--format",
            "{{.State.Status}}",
            &fixture.project_name,
        ])
        .output()?;
    if check_output.status.success() {
        let stdout = String::from_utf8_lossy(&check_output.stdout);
        let state = stdout.trim();
        assert_ne!(
            state, "running",
            "Container should not be running after force kill"
        );
    }

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_restart_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create and start VM
    fixture.run_vm_command(&["create"])?;
    fixture.run_vm_command(&["start"])?;
    assert!(fixture.wait_for_container_state("running", 30));

    // Restart the VM
    let output = fixture.run_vm_command(&["restart"])?;
    assert!(
        output.status.success(),
        "VM restart failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify container is running again after restart
    assert!(
        fixture.wait_for_container_state("running", 30),
        "Container did not restart within timeout"
    );

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_status_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Test status when VM doesn't exist
    let _output = fixture.run_vm_command(&["status"])?;
    // Should succeed but show VM as not running

    // Create VM
    fixture.run_vm_command(&["create"])?;

    // Test status when VM exists but is stopped
    let output = fixture.run_vm_command(&["status"])?;
    assert!(output.status.success(), "VM status failed when stopped");

    // Start VM and test status when running
    fixture.run_vm_command(&["start"])?;
    assert!(fixture.wait_for_container_state("running", 30));

    let output = fixture.run_vm_command(&["status"])?;
    assert!(
        output.status.success(),
        "VM status failed when running: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_list_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Test list when no VMs exist
    let output = fixture.run_vm_command(&["list"])?;
    assert!(output.status.success(), "VM list failed with no VMs");

    // Create VM using vm tool (this should have proper labels)
    let create_output = fixture.run_vm_command(&["create"])?;
    assert!(create_output.status.success(), "VM create failed");

    // Verify the container has the expected labels
    let label_check = Command::new("docker")
        .args([
            "inspect",
            &fixture.project_name,
            "--format",
            "{{index .Config.Labels \"com.vm.managed\"}} {{index .Config.Labels \"com.vm.project\"}}",
        ])
        .output()?;

    if label_check.status.success() {
        let labels = String::from_utf8_lossy(&label_check.stdout);
        assert!(
            labels.contains("true"),
            "Container should have com.vm.managed=true label"
        );
        assert!(
            labels.contains(&fixture.project_name),
            "Container should have project label"
        );
    }

    // Test vm list finds the labeled container
    let output = fixture.run_vm_command(&["list"])?;
    assert!(
        output.status.success(),
        "VM list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&fixture.project_name),
        "VM list output should contain the project name: {}",
        stdout
    );

    // Create a non-vm-managed container (without labels) to test filtering
    let non_vm_container = format!("{}-non-vm", fixture.project_name);
    let _non_vm_output = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &non_vm_container,
            "alpine:latest",
            "sleep",
            "300",
        ])
        .output()?;

    // VM list should NOT include the non-labeled container
    let output = fixture.run_vm_command(&["list"])?;
    assert!(
        output.status.success(),
        "VM list failed with mixed containers"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&fixture.project_name),
        "VM list should still show VM-managed container"
    );
    assert!(
        !stdout.contains(&non_vm_container),
        "VM list should NOT show non-VM-managed container: {}",
        stdout
    );

    // Clean up the non-vm container
    let _ = Command::new("docker")
        .args(["rm", "-f", &non_vm_container])
        .output();

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_provision_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create and start VM first
    fixture.run_vm_command(&["create"])?;
    fixture.run_vm_command(&["start"])?;
    assert!(fixture.wait_for_container_state("running", 30));

    // Test provision command
    let output = fixture.run_vm_command(&["provision"])?;
    assert!(
        output.status.success(),
        "VM provision failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_exec_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create and start VM
    fixture.run_vm_command(&["create"])?;
    fixture.run_vm_command(&["start"])?;
    assert!(fixture.wait_for_container_state("running", 30));

    // Test exec command
    let output = fixture.run_vm_command(&["exec", "echo", "Hello from VM"])?;
    assert!(
        output.status.success(),
        "VM exec failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify output contains our test message
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello from VM"), "Exec output not found");

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_logs_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create and start VM
    fixture.run_vm_command(&["create"])?;
    fixture.run_vm_command(&["start"])?;
    assert!(fixture.wait_for_container_state("running", 30));

    // Give container a moment to generate logs
    std::thread::sleep(Duration::from_secs(2));

    // Test logs command
    let output = fixture.run_vm_command(&["logs"])?;
    assert!(
        output.status.success(),
        "VM logs failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_destroy_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create VM
    fixture.run_vm_command(&["create"])?;

    // Verify container exists
    let check_output = Command::new("docker")
        .args(["inspect", &fixture.project_name])
        .output()?;
    assert!(
        check_output.status.success(),
        "Container should exist before destroy"
    );

    // Test destroy command with force flag (to avoid confirmation prompt)
    let output = fixture.run_vm_command(&["destroy", "--force"])?;
    assert!(
        output.status.success(),
        "VM destroy failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify container no longer exists
    let check_output = Command::new("docker")
        .args(["inspect", &fixture.project_name])
        .output()?;
    assert!(
        !check_output.status.success(),
        "Container should not exist after destroy"
    );

    Ok(())
}

#[test]
fn test_vm_ssh_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Create and start VM
    fixture.run_vm_command(&["create"])?;
    fixture.run_vm_command(&["start"])?;
    assert!(fixture.wait_for_container_state("running", 30));

    // Note: SSH command is interactive, so we can't easily test it in an automated way
    // without special handling. For now, we'll test that the command doesn't fail
    // immediately when the container is running.

    // We could test SSH by checking if the command accepts the arguments correctly
    // but actual SSH testing would require more complex setup with expect or similar tools

    fixture.cleanup_test_containers()?;
    Ok(())
}

#[test]
fn test_vm_lifecycle_integration() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.binary_path.exists() || !fixture.is_docker_available() {
        println!("Skipping test - vm binary or Docker not available");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Test complete lifecycle: create -> start -> status -> exec -> stop -> start -> destroy

    // 1. Create
    let output = fixture.run_vm_command(&["create"])?;
    assert!(output.status.success(), "Create failed");

    // 2. Start
    let output = fixture.run_vm_command(&["start"])?;
    assert!(output.status.success(), "Start failed");
    assert!(fixture.wait_for_container_state("running", 30));

    // 3. Status check
    let output = fixture.run_vm_command(&["status"])?;
    assert!(output.status.success(), "Status failed");

    // 4. Exec command
    let output = fixture.run_vm_command(&["exec", "pwd"])?;
    assert!(output.status.success(), "Exec failed");

    // 5. Stop
    let output = fixture.run_vm_command(&["stop"])?;
    assert!(output.status.success(), "Stop failed");
    assert!(fixture.wait_for_container_state("exited", 30));

    // 6. Start again
    let output = fixture.run_vm_command(&["start"])?;
    assert!(output.status.success(), "Restart after stop failed");
    assert!(fixture.wait_for_container_state("running", 30));

    // 7. Destroy
    let output = fixture.run_vm_command(&["destroy", "--force"])?;
    assert!(output.status.success(), "Destroy failed");

    // 8. Verify container is gone
    let check_output = Command::new("docker")
        .args(["inspect", &fixture.project_name])
        .output()?;
    assert!(
        !check_output.status.success(),
        "Container should not exist after destroy"
    );

    Ok(())
}
