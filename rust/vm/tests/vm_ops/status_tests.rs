use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;
use std::process::Command;

#[test]
fn test_vm_status_command() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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
