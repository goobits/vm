//! # Input Validation: Docker
//!
//! This module provides validation helpers for Docker-related inputs such as
//! container names, image names, ports, and volume paths.

use crate::validation::error::ValidationError;
use crate::validation::result::ValidationResult;
use once_cell::sync::Lazy;
use regex::Regex;

/// Regex for validating Docker container/image names
static DOCKER_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._-]*$")
        .expect("Docker name regex should compile - this is a static pattern")
});

/// Sanitize and validate Docker container/image names.
///
/// This function ensures Docker names follow the proper naming conventions
/// and don't contain malicious characters that could be used for injection attacks.
/// Docker names must start with an alphanumeric character and can contain
/// letters, numbers, dots, hyphens, and underscores.
///
/// # Arguments
///
/// * `name` - The Docker name to sanitize
///
/// # Returns
///
/// `Ok(String)` with the sanitized name, `Err(ValidationError)` if invalid
///
/// # Examples
///
/// ```rust
/// use vm_package_server::validation::docker::sanitize_docker_name;
///
/// // Valid Docker name
/// // assert!(sanitize_docker_name("my-app-container").is_ok());
///
/// // Invalid characters
/// // assert!(sanitize_docker_name("my app container").is_err());
/// ```
pub fn sanitize_docker_name(name: &str) -> ValidationResult<String> {
    // Check length
    if name.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if name.len() > 253 {
        return Err(ValidationError::TooLong {
            actual: name.len(),
            max: 253,
        });
    }

    // Check for null bytes
    if name.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    // Check for control characters
    if name.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Validate Docker naming convention
    if !DOCKER_NAME_REGEX.is_match(name) {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    Ok(name.to_string())
}

/// Validate and sanitize Docker port numbers.
///
/// This function validates port numbers to ensure they are within valid ranges
/// and safe for use in Docker commands.
///
/// # Arguments
///
/// * `port` - The port number to validate
///
/// # Returns
///
/// `Ok(u16)` if the port is valid, `Err(ValidationError)` otherwise
pub fn validate_docker_port(port: u16) -> ValidationResult<u16> {
    // Docker ports must be in the range 1-65535
    // Port 0 is invalid for Docker port mapping
    if port == 0 {
        return Err(ValidationError::InvalidFormat {
            reason: "Port 0 is not valid for Docker port mapping".to_string(),
        });
    }

    // Ports 1-1023 are privileged ports, warn but allow
    if port < 1024 {
        // Allow but this might require root privileges
    }

    Ok(port)
}

/// Validate and sanitize Docker image names.
///
/// This function validates Docker image names to ensure they follow Docker
/// naming conventions and don't contain injection patterns.
///
/// # Arguments
///
/// * `image_name` - The Docker image name to validate
///
/// # Returns
///
/// `Ok(String)` with the validated image name, `Err(ValidationError)` if invalid
pub fn validate_docker_image_name(image_name: &str) -> ValidationResult<String> {
    if image_name.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if image_name.len() > 255 {
        return Err(ValidationError::TooLong {
            actual: image_name.len(),
            max: 255,
        });
    }

    // Check for null bytes and control characters
    if image_name.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    if image_name.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Docker image names can contain:
    // - lowercase letters, numbers, hyphens, underscores, periods
    // - optional registry hostname (with dots and colons)
    // - optional tag (after colon)
    // - optional digest (after @)

    // Basic validation for dangerous characters
    let dangerous_chars = [
        '`', '$', '&', '|', ';', '<', '>', '(', ')', '{', '}', '[', ']', '\\', '"', '\'', ' ', '\t',
    ];
    for ch in dangerous_chars {
        if image_name.contains(ch) {
            return Err(ValidationError::InvalidCharacters {
                input: image_name.to_string(),
            });
        }
    }

    // Validate format more strictly
    // Allow registry/namespace/name:tag format
    if !image_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: image_name.to_string(),
        });
    }

    Ok(image_name.to_string())
}

/// Validate absolute paths for Docker volume mounting.
///
/// This function validates absolute paths to ensure they are safe for use
/// in Docker volume mounts and don't contain injection patterns.
///
/// # Arguments
///
/// * `path` - The absolute path to validate
///
/// # Returns
///
/// `Ok(String)` with the validated path, `Err(ValidationError)` if invalid
pub fn validate_docker_volume_path<P: AsRef<std::path::Path>>(path: P) -> ValidationResult<String> {
    let path = path.as_ref();
    let path_str = path.to_string_lossy();

    // Check for null bytes
    if path_str.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    // Check for control characters
    if path_str
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
    {
        return Err(ValidationError::ControlCharacters);
    }

    // Must be absolute path for Docker volumes
    if !path.is_absolute() {
        return Err(ValidationError::InvalidFormat {
            reason: "Docker volume paths must be absolute".to_string(),
        });
    }

    // Check for dangerous shell characters that could cause injection
    let dangerous_patterns = [
        "`", "$", "&", "|", ";", "<", ">", "(", ")", "{", "}", "[", "]",
    ];
    for pattern in &dangerous_patterns {
        if path_str.contains(pattern) {
            return Err(ValidationError::InvalidCharacters {
                input: path_str.to_string(),
            });
        }
    }

    Ok(path_str.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_docker_name() {
        assert_eq!(
            sanitize_docker_name("my-app-container").unwrap(),
            "my-app-container"
        );
        assert_eq!(sanitize_docker_name("app123").unwrap(), "app123");

        assert!(sanitize_docker_name("").is_err());
        assert!(sanitize_docker_name("my app container").is_err());
        assert!(sanitize_docker_name("-invalid-start").is_err());
    }

    #[test]
    fn test_validate_docker_port() {
        // Valid ports
        assert!(validate_docker_port(80).is_ok());
        assert!(validate_docker_port(8080).is_ok());
        assert!(validate_docker_port(65535).is_ok());

        // Invalid port 0
        assert!(validate_docker_port(0).is_err());
    }

    #[test]
    fn test_validate_docker_image_name() {
        // Valid image names
        assert!(validate_docker_image_name("nginx").is_ok());
        assert!(validate_docker_image_name("nginx:latest").is_ok());
        assert!(validate_docker_image_name("registry.example.com/namespace/image:tag").is_ok());
        assert!(validate_docker_image_name("my-app-123").is_ok());

        // Invalid image names
        assert!(validate_docker_image_name("").is_err());
        assert!(validate_docker_image_name("image with spaces").is_err());
        assert!(validate_docker_image_name("image$with$dollars").is_err());
        assert!(validate_docker_image_name("image`with`backticks").is_err());
        assert!(validate_docker_image_name("image;with;semicolons").is_err());
        assert!(validate_docker_image_name("image&with&ampersands").is_err());
    }

    #[test]
    fn test_validate_docker_volume_path() {
        // Valid absolute paths
        assert!(validate_docker_volume_path("/home/user/data").is_ok());
        assert!(validate_docker_volume_path("/var/lib/app").is_ok());

        // Invalid relative path
        assert!(validate_docker_volume_path("relative/path").is_err());
        assert!(validate_docker_volume_path("./relative/path").is_err());
        assert!(validate_docker_volume_path("../parent/path").is_err());

        // Invalid paths with injection characters
        assert!(validate_docker_volume_path("/path/with$dollar").is_err());
        assert!(validate_docker_volume_path("/path/with`backtick").is_err());
        assert!(validate_docker_volume_path("/path/with;semicolon").is_err());
        assert!(validate_docker_volume_path("/path/with&ampersand").is_err());
        assert!(validate_docker_volume_path("/path/with|pipe").is_err());

        // Path with null bytes
        assert!(validate_docker_volume_path("/path/with\0null").is_err());
    }
}