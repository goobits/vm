use vm_docker_registry::server;

#[tokio::test]
#[cfg(feature = "integration")]
async fn test_docker_registry_lifecycle_integration() {
    // This test requires Docker to be running.
    if !is_docker_available().await {
        println!("Skipping test: Docker is not available.");
        return;
    }

    // Stop the registry first to ensure a clean state
    let stop_result = server::stop_registry().await;
    assert!(stop_result.is_ok(), "Should be able to stop the registry even if it's not running");

    // Start the registry
    let start_result = server::start_registry().await;
    assert!(start_result.is_ok(), "Failed to start the Docker registry");

    // Check if the registry is running
    let is_running = server::check_registry_running(vm_docker_registry::DEFAULT_REGISTRY_PORT).await;
    assert!(is_running, "Registry should be running after start");

    // Stop the registry
    let stop_result = server::stop_registry().await;
    assert!(stop_result.is_ok(), "Failed to stop the Docker registry");

    // Check if the registry is stopped
    let is_running = server::check_registry_running(vm_docker_registry::DEFAULT_REGISTRY_PORT).await;
    assert!(!is_running, "Registry should be stopped after stop");
}

async fn is_docker_available() -> bool {
    let output = tokio::process::Command::new("docker")
        .arg("version")
        .output()
        .await;

    match output {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}
