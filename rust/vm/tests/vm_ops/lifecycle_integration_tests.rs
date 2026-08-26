use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;
use std::process::Command;

#[test]
#[ignore = "Creates real Docker containers; run with --ignored"]
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

    // Test complete lifecycle: run -> status -> exec -> stop -> start -> remove

    // 1. Create and start
    let output = fixture.run_vm_command(&["run", "container"])?;
    assert!(output.status.success(), "Run failed");
    assert!(fixture.wait_for_container_state("running", 30));

    // 2. Status check
    let output = fixture.run_vm_command(&["status"])?;
    assert!(output.status.success(), "Status failed");

    // 3. Exec command
    let output = fixture.run_vm_command(&["exec", "pwd"])?;
    assert!(output.status.success(), "Exec failed");

    // 4. Stop
    let output = fixture.run_vm_command(&["stop"])?;
    assert!(output.status.success(), "Stop failed");
    assert!(fixture.wait_for_container_state("exited", 30));

    // 5. Start again
    let output = fixture.run_vm_command(&["start"])?;
    assert!(output.status.success(), "Restart after stop failed");
    assert!(fixture.wait_for_container_state("running", 30));

    // 6. Remove
    let output = fixture.run_vm_command(&["remove", "--force"])?;
    assert!(output.status.success(), "Remove failed");

    // 7. Verify container is gone
    let check_output = Command::new("docker")
        .args(["inspect", &fixture.project_name])
        .output()?;
    assert!(
        !check_output.status.success(),
        "Container should not exist after destroy"
    );

    Ok(())
}
