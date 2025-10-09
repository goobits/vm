use anyhow::Result;
use assert_cmd::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;

// Global mutex to ensure tests run sequentially to avoid Docker container conflicts
pub static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Test fixture for VM operations integration testing
///
/// This fixture creates isolated temporary directories and ensures all VM operations
/// are executed in completely isolated environments that don't affect the project filesystem.
pub struct VmOpsTestFixture {
    _temp_dir: TempDir,
    pub test_dir: PathBuf,
    pub binary_path: PathBuf,
    pub project_name: String,
}

impl VmOpsTestFixture {
    pub fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_dir)?;

        // Generate unique project name to avoid Docker container conflicts
        let uuid_str = uuid::Uuid::new_v4().to_string();
        let project_name = format!("vm-test-{}", &uuid_str[..8]);

        // Get the path to the vm binary using the helper
        let binary_path = binary_path()?;

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            binary_path,
            project_name,
        })
    }

    /// Run vm command with given arguments in the isolated test directory
    pub fn run_vm_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(&self.binary_path)
            .args(args)
            .current_dir(&self.test_dir)
            .env("HOME", self.test_dir.parent().unwrap())
            .env("VM_TOOL_DIR", &self.test_dir)
            .env("VM_TEST_MODE", "1") // Disable structured logging for tests
            .output()?;
        Ok(output)
    }

    /// Create a vm.yaml file for a specific provider
    pub fn create_config_for_provider(&self, provider: &str) -> Result<()> {
        let config_content = format!(
            r#"provider: {}
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
            provider, self.project_name
        );

        fs::write(self.test_dir.join("vm.yaml"), config_content)?;
        Ok(())
    }

    /// Create a minimal vm.yaml configuration for testing (defaults to Docker)
    pub fn create_test_config(&self) -> Result<()> {
        self.create_config_for_provider("docker")
    }

    /// Create a Dockerfile for testing provision operations
    pub fn create_test_dockerfile(&self) -> Result<()> {
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
    pub fn is_docker_available(&self) -> bool {
        Command::new("docker")
            .args(["info"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Check if Tart is available and working
    pub fn is_tart_available(&self) -> bool {
        if !cfg!(target_os = "macos") {
            return false;
        }
        Command::new("tart")
            .args(["--version"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Clean up any existing test containers
    pub fn cleanup_test_containers(&self) -> Result<()> {
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
    pub fn wait_for_container_state(&self, expected_state: &str, timeout_secs: u64) -> bool {
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
    pub fn check_container_state(&self, expected_state: &str) -> bool {
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

/// Resolve the path to the `vm` binary for integration testing.
///
/// This function uses `assert_cmd` to locate the binary for the `vm` package,
/// ensuring a reliable path for integration tests.
pub fn binary_path() -> Result<PathBuf> {
    let cmd = Command::cargo_bin("vm")?;
    Ok(PathBuf::from(cmd.get_program()))
}
