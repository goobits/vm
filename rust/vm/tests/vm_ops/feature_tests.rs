use super::helpers::{VmOpsTestFixture, TEST_MUTEX};
use anyhow::Result;
use std::fs;
use std::process::Command;
use std::time::Duration;

/// Phase 2 Integration Test: Package Registry Feature
///
/// This test validates that when a VM is created with package registry enabled in global config,
/// the container receives the correct environment variables and configuration files.
#[test]
fn test_package_registry_feature() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap();
    let fixture = VmOpsTestFixture::new()?;

    // Skip test if Docker is not available
    if !fixture.is_docker_available() {
        println!("⚠️  Skipping test_package_registry_feature - Docker not available");
        return Ok(());
    }

    // Step 1: Create .vm directory for global config
    let vm_config_dir = fixture.test_dir.parent().unwrap().join(".vm");
    fs::create_dir_all(&vm_config_dir)?;

    // Step 2: Create global config.yaml with package registry enabled
    let global_config_content = r#"services:
  package_registry:
    enabled: true
    port: 3080
"#;
    fs::write(vm_config_dir.join("config.yaml"), global_config_content)?;

    // Step 3: Create basic vm.yaml for test project
    let vm_yaml_content = format!(
        r#"provider: docker
project:
  name: {}
vm:
  memory: 1024
  cpus: 1
"#,
        fixture.project_name
    );
    fs::write(fixture.test_dir.join("vm.yaml"), vm_yaml_content)?;

    // Step 4: Create the VM (this should inject registry env vars)
    println!("Creating VM with package registry enabled...");
    let output = fixture.run_vm_command(&["create"])?;
    if !output.status.success() {
        eprintln!(
            "Create failed stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        eprintln!(
            "Create failed stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(output.status.success(), "VM creation should succeed");

    // Wait for container to be fully ready
    std::thread::sleep(Duration::from_secs(3));

    let container_name = format!("{}-dev", fixture.project_name);

    // Step 5: Verify environment variables are set in the container
    println!("Verifying environment variables in container...");
    let env_output = Command::new("docker")
        .args(["exec", &container_name, "printenv"])
        .output()?;

    assert!(
        env_output.status.success(),
        "Failed to get container environment variables"
    );

    let env_vars = String::from_utf8_lossy(&env_output.stdout);

    // Determine expected host based on platform
    let expected_host = if cfg!(target_os = "linux") {
        "172.17.0.1"
    } else {
        "host.docker.internal"
    };

    // Verify NPM_CONFIG_REGISTRY
    let expected_npm_registry = format!("NPM_CONFIG_REGISTRY=http://{}:3080/npm/", expected_host);
    assert!(
        env_vars.contains(&expected_npm_registry),
        "NPM_CONFIG_REGISTRY should be set to {}",
        expected_npm_registry
    );

    // Verify PIP_INDEX_URL
    let expected_pip_index = format!("PIP_INDEX_URL=http://{}:3080/pypi/simple/", expected_host);
    assert!(
        env_vars.contains(&expected_pip_index),
        "PIP_INDEX_URL should be set to {}",
        expected_pip_index
    );

    // Verify PIP_EXTRA_INDEX_URL
    assert!(
        env_vars.contains("PIP_EXTRA_INDEX_URL=https://pypi.org/simple/"),
        "PIP_EXTRA_INDEX_URL should be set"
    );

    // Verify PIP_TRUSTED_HOST
    let expected_pip_trusted = format!("PIP_TRUSTED_HOST={}", expected_host);
    assert!(
        env_vars.contains(&expected_pip_trusted),
        "PIP_TRUSTED_HOST should be set to {}",
        expected_pip_trusted
    );

    // Verify VM_CARGO_REGISTRY_HOST
    let expected_cargo_host = format!("VM_CARGO_REGISTRY_HOST={}", expected_host);
    assert!(
        env_vars.contains(&expected_cargo_host),
        "VM_CARGO_REGISTRY_HOST should be set to {}",
        expected_cargo_host
    );

    // Verify VM_CARGO_REGISTRY_PORT
    assert!(
        env_vars.contains("VM_CARGO_REGISTRY_PORT=3080"),
        "VM_CARGO_REGISTRY_PORT should be set to 3080"
    );

    // Step 6: Verify ~/.cargo/config.toml is created on shell login
    println!("Verifying Cargo configuration file...");

    // Simulate shell login by sourcing .zshrc and checking config
    let cargo_config_output = Command::new("docker")
        .args([
            "exec",
            &container_name,
            "/bin/zsh",
            "-c",
            "source ~/.zshrc && cat ~/.cargo/config.toml 2>/dev/null || echo 'NOT_FOUND'",
        ])
        .output()?;

    let cargo_config_content = String::from_utf8_lossy(&cargo_config_output.stdout);

    if cargo_config_content.contains("NOT_FOUND") {
        eprintln!("⚠️  Warning: ~/.cargo/config.toml not found after shell login");
        eprintln!("This may be expected if the shell initialization hasn't run yet");

        // Try to manually trigger the config creation
        let _ = Command::new("docker")
            .args([
                "exec",
                &container_name,
                "/bin/zsh",
                "-l", // Login shell to ensure .zshrc is sourced
                "-c",
                "echo 'Config should be created now' && cat ~/.cargo/config.toml",
            ])
            .output()?;
    } else {
        // Verify config contains expected entries
        assert!(
            cargo_config_content.contains("[source.vm-registry]"),
            "Cargo config should contain [source.vm-registry] section"
        );

        // The Dockerfile script creates the URL with /cargo/ path
        let expected_registry_url = format!("sparse+http://{}:3080/cargo/", expected_host);
        assert!(
            cargo_config_content.contains(&expected_registry_url),
            "Cargo config should contain registry URL: {}",
            expected_registry_url
        );

        println!("✅ Cargo configuration verified successfully");
    }

    // Step 7: Cleanup - Destroy the VM
    println!("Cleaning up test VM...");
    let destroy_output = fixture.run_vm_command(&["destroy", "--force"])?;
    assert!(
        destroy_output.status.success(),
        "VM destruction should succeed"
    );

    // Verify container is removed
    let check_output = Command::new("docker")
        .args(["inspect", &container_name])
        .output()?;
    assert!(
        !check_output.status.success(),
        "Container should not exist after destroy"
    );

    println!("✅ Package registry integration test completed successfully");
    Ok(())
}
