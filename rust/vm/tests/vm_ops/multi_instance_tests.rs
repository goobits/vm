use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;

#[test]
fn test_vm_multi_instance_lifecycle() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();

    // Create two separate test fixtures for isolation
    let fixture1 = VmOpsTestFixture::new()?;
    let fixture2 = VmOpsTestFixture::new()?;

    if !fixture1.is_docker_available() {
        println!("Skipping multi-instance test: Docker not available");
        return Ok(());
    }

    // Create and start the first VM
    fixture1.create_test_config()?;
    assert!(fixture1.run_vm_command(&["create", "--force"])?.status.success());
    assert!(fixture1.run_vm_command(&["start"])?.status.success());

    // Create and start the second VM
    fixture2.create_test_config()?;
    assert!(fixture2.run_vm_command(&["create", "--force"])?.status.success());
    assert!(fixture2.run_vm_command(&["start"])?.status.success());

    // Verify both VMs are listed as running
    let list_output = fixture1.run_vm_command(&["list"])?;
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        list_stdout.contains(&fixture1.project_name) && list_stdout.contains("running"),
        "Instance 1 not listed as running"
    );
    assert!(
        list_stdout.contains(&fixture2.project_name) && list_stdout.contains("running"),
        "Instance 2 not listed as running"
    );

    // Stop the first VM and verify the second is unaffected
    assert!(fixture1.run_vm_command(&["stop"])?.status.success());

    let status1_output = fixture1.run_vm_command(&["status"])?;
    assert!(String::from_utf8_lossy(&status1_output.stdout).contains("stopped"), "Instance 1 should be stopped");

    let status2_output = fixture2.run_vm_command(&["status"])?;
    assert!(String::from_utf8_lossy(&status2_output.stdout).contains("running"), "Instance 2 should still be running");

    // Clean up both VMs
    assert!(fixture1.run_vm_command(&["destroy", "--force"])?.status.success());
    assert!(fixture2.run_vm_command(&["destroy", "--force"])?.status.success());

    Ok(())
}