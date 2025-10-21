#[cfg(feature = "integration")]
use anyhow::Result;
#[cfg(feature = "integration")]
use std::fs;
#[cfg(feature = "integration")]
use std::path::PathBuf;
#[cfg(feature = "integration")]
use std::process::Command;
#[cfg(feature = "integration")]
use tempfile::TempDir;

/// Test fixture for end-to-end CLI workflow testing
#[cfg(feature = "integration")]
struct TempWorkflowTestFixture {
    _temp_dir: TempDir,
    test_dir: PathBuf,
    binary_path: PathBuf,
    mount_dir: PathBuf,
}

#[cfg(feature = "integration")]
impl TempWorkflowTestFixture {
    fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_project");
        fs::create_dir_all(&test_dir)?;
        let mount_dir = test_dir.join("mount_me");
        fs::create_dir_all(&mount_dir)?;

        // Get the path to the vm binary
        let workspace_root = std::env::current_dir()?.parent().unwrap().to_path_buf();
        let binary_path = workspace_root.join("target").join("debug").join("vm");

        Ok(Self {
            _temp_dir: temp_dir,
            test_dir,
            binary_path,
            mount_dir,
        })
    }

    /// Run vm command with given arguments in the test directory
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

    /// Check if a file exists in the test directory
    fn state_file_exists(&self) -> bool {
        self.test_dir.join(".vm-temp-state.yaml").exists()
    }

    /// Get the contents of a file as a string
    fn read_state_file(&self) -> Result<String> {
        let path = self.test_dir.join(".vm-temp-state.yaml");
        Ok(fs::read_to_string(path)?)
    }
    /// Create a vm.yaml file in the test directory
    fn create_config_file(&self, content: &str) -> Result<()> {
        let path = self.test_dir.join("vm.yaml");
        fs::write(path, content)?;
        Ok(())
    }
}

/// Check if Docker is available and working on the system
#[cfg(feature = "integration")]
fn is_docker_available() -> bool {
    // Check if docker command exists
    let docker_version = Command::new("docker")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !docker_version {
        return false;
    }

    // Check if docker compose is available
    let compose_available = Command::new("docker")
        .args(["compose", "version"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    if !compose_available {
        return false;
    }

    // Check if Docker daemon is actually running by trying a simple command
    let daemon_running = Command::new("docker")
        .args(["info"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);

    daemon_running
}

#[test]
#[cfg(feature = "integration")]
fn test_temp_vm_full_lifecycle() -> Result<()> {
    let fixture = TempWorkflowTestFixture::new()?;

    // Skip test if binary doesn't exist
    if !fixture.binary_path.exists() {
        println!(
            "Skipping test - vm binary not found at {:?}",
            fixture.binary_path
        );
        return Ok(());
    }

    // Skip test if Docker is not available
    if !is_docker_available() {
        println!("Skipping test - Docker not available for integration testing");
        return Ok(());
    }

    println!("Docker is available, proceeding with integration test...");

    // Use the docker provider for this test
    fixture.create_config_file("provider: docker")?;

    // 1. Create a temp VM
    let output = fixture.run_vm_command(&["temp", "create", "./"])?;
    assert!(
        output.status.success(),
        "Failed to create temp vm: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fixture.state_file_exists(), "State file was not created");

    // 2. Add a mount
    let mount_path_str = fixture.mount_dir.to_str().unwrap();
    let output = fixture.run_vm_command(&["temp", "mount", mount_path_str, "--yes"])?;
    assert!(
        output.status.success(),
        "Failed to mount directory: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify mount was added to state file
    let state_content = fixture.read_state_file()?;
    assert!(
        state_content.contains(mount_path_str),
        "Mount path not found in state file"
    );

    // 3. List mounts
    let output = fixture.run_vm_command(&["temp", "mounts"])?;
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout)?;
    assert!(
        stdout.contains(mount_path_str),
        "List mounts did not show the correct path"
    );

    // 4. Unmount the directory
    let output = fixture.run_vm_command(&["temp", "unmount", mount_path_str, "--yes"])?;
    assert!(
        output.status.success(),
        "Failed to unmount directory: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let state_content_after_unmount = fixture.read_state_file()?;
    assert!(
        !state_content_after_unmount.contains(mount_path_str),
        "Mount path was not removed from state file"
    );

    // 5. Destroy the temp VM
    let output = fixture.run_vm_command(&["temp", "destroy"])?;
    assert!(
        output.status.success(),
        "Failed to destroy temp vm: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !fixture.state_file_exists(),
        "State file was not deleted after destroy"
    );

    Ok(())
}
