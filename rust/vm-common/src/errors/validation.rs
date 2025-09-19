//! Configuration validation error handling

use crate::{vm_error, vm_error_hint};

/// Handle missing required field error
pub fn missing_required_field(field: &str) -> anyhow::Error {
    vm_error!("Missing required field: {}", field);
    vm_error_hint!("Add the '{}' field to your configuration", field);
    anyhow::anyhow!("Missing required field: {}", field)
}

/// Handle invalid provider error
pub fn invalid_provider(provider: &str) -> anyhow::Error {
    vm_error!(
        "Invalid provider: {}. Must be one of: docker, vagrant, tart",
        provider
    );
    vm_error_hint!("Use 'docker' for most use cases");
    anyhow::anyhow!("Invalid provider")
}

/// Handle invalid project name error
pub fn invalid_project_name(name: &str) -> anyhow::Error {
    vm_error!("Invalid project name: {}. Must contain only alphanumeric characters, dashes, and underscores", name);
    vm_error_hint!("Example: 'my-awesome-project' or 'web_app_v2'");
    anyhow::anyhow!("Invalid project name")
}

/// Handle invalid port number error
pub fn invalid_port(port: u16, reason: &str) -> anyhow::Error {
    vm_error!("Invalid port {}: {}", port, reason);
    vm_error_hint!("Use ports between 1024-65535 for user applications");
    anyhow::anyhow!("Invalid port")
}

/// Handle port already in use error (different from network.rs - this is validation)
pub fn port_already_configured(port: u16, service: &str) -> anyhow::Error {
    vm_error!(
        "Port {} is already configured for service '{}'",
        port,
        service
    );
    vm_error_hint!("Choose a different port or remove the conflicting service");
    anyhow::anyhow!("Port already configured")
}

/// Handle invalid GPU type error
pub fn invalid_gpu_type(gpu_type: &str) -> anyhow::Error {
    vm_error!(
        "Invalid GPU type: {}. Must be one of: nvidia, amd, intel, auto",
        gpu_type
    );
    vm_error_hint!("Use 'auto' to detect automatically");
    anyhow::anyhow!("Invalid GPU type")
}

/// Handle invalid service configuration error
pub fn invalid_service_config(service: &str, reason: &str) -> anyhow::Error {
    vm_error!(
        "Invalid configuration for service '{}': {}",
        service,
        reason
    );
    vm_error_hint!("Check the service documentation for valid configuration options");
    anyhow::anyhow!("Invalid service configuration")
}

/// Handle invalid version format error
pub fn invalid_version_format(version: &str, expected_format: &str) -> anyhow::Error {
    vm_error!(
        "Invalid version format '{}'. Expected format: {}",
        version,
        expected_format
    );
    vm_error_hint!("Example: '1.2.3' or '>=1.0.0'");
    anyhow::anyhow!("Invalid version format")
}

/// Handle conflicting configuration options error
pub fn conflicting_options(option1: &str, option2: &str) -> anyhow::Error {
    vm_error!(
        "Conflicting configuration options: '{}' and '{}' cannot both be enabled",
        option1,
        option2
    );
    vm_error_hint!("Choose one option and remove the other");
    anyhow::anyhow!("Conflicting configuration options")
}

/// Handle invalid memory specification error
pub fn invalid_memory_spec(spec: &str) -> anyhow::Error {
    vm_error!("Invalid memory specification: {}", spec);
    vm_error_hint!("Use format like '4GB', '2048MB', or 'unlimited'");
    anyhow::anyhow!("Invalid memory specification")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_missing_required_field() {
        let err = missing_required_field("provider");
        assert!(err.to_string().contains("Missing required field: provider"));
    }

    #[test]
    fn test_invalid_provider() {
        let err = invalid_provider("invalid");
        assert!(err.to_string().contains("Invalid provider"));
    }

    #[test]
    fn test_invalid_project_name() {
        let err = invalid_project_name("invalid name!");
        assert!(err.to_string().contains("Invalid project name"));
    }

    #[test]
    fn test_invalid_gpu_type() {
        let err = invalid_gpu_type("invalid");
        assert!(err.to_string().contains("Invalid GPU type"));
    }
}
