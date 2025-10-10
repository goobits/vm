use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;

#[test]
fn test_vm_lifecycle_provider_parity() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;
    fixture.create_test_dockerfile()?;

    let providers = ["docker", "tart"];

    for &provider in &providers {
        println!("Testing provider: {}", provider);

        if provider == "docker" && !fixture.is_docker_available() {
            println!("Skipping Docker test: Docker not available");
            continue;
        }

        if provider == "tart" && !fixture.is_tart_available() {
            println!("Skipping Tart test: Tart not available");
            continue;
        }

        fixture.create_config_for_provider(provider)?;

        // Test `vm create`
        let output = fixture.run_vm_command(&["create", "--force"])?;
        assert!(output.status.success(), "vm create failed for {}", provider);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("VM created successfully"),
            "vm create output was incorrect for {}: {}",
            provider,
            stdout
        );

        // Test `vm start`
        let output = fixture.run_vm_command(&["start"])?;
        assert!(output.status.success(), "vm start failed for {}", provider);

        // Test `vm status`
        let output = fixture.run_vm_command(&["status"])?;
        assert!(output.status.success(), "vm status failed for {}", provider);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("running"),
            "vm status should show running for {}",
            provider
        );

        // Test `vm stop`
        let output = fixture.run_vm_command(&["stop"])?;
        assert!(output.status.success(), "vm stop failed for {}", provider);

        // Test `vm destroy`
        let output = fixture.run_vm_command(&["destroy", "--force"])?;
        assert!(
            output.status.success(),
            "vm destroy failed for {}",
            provider
        );
    }

    Ok(())
}
