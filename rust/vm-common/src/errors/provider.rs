//! Provider-related error handling

use crate::{vm_error, vm_error_hint, vm_error_with_details};

/// Handle Docker daemon connection error
pub fn docker_connection_failed() -> anyhow::Error {
    vm_error_with_details!(
        "Failed to connect to Docker daemon",
        &[
            "Docker service may not be running",
            "Check if Docker Desktop is started",
            "Verify Docker installation"
        ]
    );
    vm_error_hint!("Try: docker info");
    anyhow::anyhow!("Docker daemon connection failed")
}

/// Handle container creation failure
pub fn container_creation_failed(reason: &str) -> anyhow::Error {
    vm_error!("Failed to create container: {}", reason);
    vm_error_hint!("Check Docker logs for more details: docker logs <container-name>");
    anyhow::anyhow!("Container creation failed")
}

/// Handle provider not found error
pub fn provider_not_found(provider: &str) -> anyhow::Error {
    vm_error!("Unknown provider: {}", provider);
    vm_error_hint!("Supported providers: docker, vagrant, tart");
    anyhow::anyhow!("Provider not found: {}", provider)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_docker_connection_failed() {
        let err = docker_connection_failed();
        assert!(err.to_string().contains("Docker daemon connection failed"));
    }
}
