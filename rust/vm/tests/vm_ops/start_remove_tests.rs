use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;
use std::process::Command;

#[test]
#[ignore = "Creates real Docker containers; run with --ignored"]
fn test_vm_start_from_config_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available for integration testing");
        return Ok(());
    }

    fixture.cleanup_test_containers()?;
    fixture.create_test_config()?;
    fixture.create_test_dockerfile()?;

    // Test VM creation and startup
    let output = fixture.run_vm_command(&["start"])?;
    assert!(
        output.status.success(),
        "VM start failed: {}",
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
#[ignore = "Creates real Docker containers; run with --ignored"]
fn test_vm_remove_command() -> Result<()> {
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
    fixture.run_vm_command(&["start"])?;

    // Verify container exists
    let check_output = Command::new("docker")
        .args(["inspect", &fixture.project_name])
        .output()?;
    assert!(
        check_output.status.success(),
        "Container should exist before removal"
    );

    // Test remove command with force flag (to avoid confirmation prompt)
    let output = fixture.run_vm_command(&["remove", "--yes"])?;
    assert!(
        output.status.success(),
        "VM remove failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify container no longer exists
    let check_output = Command::new("docker")
        .args(["inspect", &fixture.project_name])
        .output()?;
    assert!(
        !check_output.status.success(),
        "Container should not exist after removal"
    );

    Ok(())
}
