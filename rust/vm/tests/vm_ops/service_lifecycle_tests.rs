use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;
use std::process::Command;
use std::time::Duration;

#[test]
fn test_vm_start_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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
fn test_vm_provision_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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
