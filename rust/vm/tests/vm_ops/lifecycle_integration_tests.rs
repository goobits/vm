use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;
use std::process::Command;

#[test]
fn test_vm_lifecycle_integration() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    if !fixture.is_docker_available() {
        println!("Skipping test - Docker not available");
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
