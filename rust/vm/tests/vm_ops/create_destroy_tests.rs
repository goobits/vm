use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;
use std::process::Command;

#[test]
fn test_vm_create_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

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

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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
fn test_vm_destroy_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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