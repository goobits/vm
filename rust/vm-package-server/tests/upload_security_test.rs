//! Upload Security Tests
//!
//! This module contains comprehensive security tests for the validation module.
//! Tests verify that security functions properly prevent DoS attacks, injection
//! attacks, and other security vulnerabilities in upload handling.

use vm_package_server::validation;

#[cfg(test)]
mod validation_security_tests {
    use super::*;

    #[test]
    fn test_base64_size_validation_prevents_bombs() {
        // Test normal valid base64
        let valid_base64 = "SGVsbG8gV29ybGQ="; // "Hello World"
        assert!(validation::validate_base64_size(valid_base64, None, None).is_ok());

        // Test oversized base64 (simulated large string)
        let large_base64 = "A".repeat(validation::MAX_BASE64_ENCODED_SIZE + 1);
        assert!(validation::validate_base64_size(&large_base64, None, None).is_err());
    }

    #[test]
    fn test_docker_name_sanitization() {
        // Valid Docker names should pass
        assert!(validation::sanitize_docker_name("valid-container").is_ok());
        assert!(validation::sanitize_docker_name("test123").is_ok());

        // Invalid Docker names should fail
        assert!(validation::sanitize_docker_name("../malicious").is_err());
        assert!(validation::sanitize_docker_name("container; rm -rf /").is_err());
        assert!(validation::sanitize_docker_name("").is_err());
    }

    #[test]
    fn test_file_size_validation() {
        // Valid file sizes should pass
        assert!(validation::validate_file_size(1024, None).is_ok());
        assert!(validation::validate_file_size(1024 * 1024, None).is_ok()); // 1MB

        // Oversized files should fail
        assert!(validation::validate_file_size(validation::MAX_UPLOAD_SIZE + 1, None).is_err());
    }

    #[test]
    fn test_hostname_validation() {
        // Valid hostnames should pass
        assert!(validation::validate_hostname("localhost").is_ok());
        assert!(validation::validate_hostname("example.com").is_ok());
        assert!(validation::validate_hostname("sub.example.com").is_ok());

        // Invalid hostnames should fail
        assert!(validation::validate_hostname("").is_err());
        assert!(validation::validate_hostname("host with spaces").is_err());
        assert!(validation::validate_hostname("host;malicious").is_err());
    }

    #[test]
    fn test_safe_path_validation() {
        // Valid relative paths should pass
        assert!(validation::validate_safe_path("safe/path/file.txt").is_ok());
        assert!(validation::validate_safe_path("package/version/file").is_ok());

        // Dangerous paths should fail
        assert!(validation::validate_safe_path("../../../etc/passwd").is_err());
        assert!(validation::validate_safe_path("/absolute/path").is_err());
        assert!(validation::validate_safe_path("path\0injection").is_err());
    }

    #[test]
    fn test_shell_arg_escaping() {
        // Test that shell arguments are properly escaped
        let safe_arg = validation::escape_shell_arg("safe-filename");
        // Safe filenames might not need quoting
        assert!(!safe_arg.is_empty());

        let dangerous_arg = validation::escape_shell_arg("file; rm -rf /");
        // Dangerous args should be quoted or escaped
        assert!(dangerous_arg.contains("'") || dangerous_arg.len() > "file; rm -rf /".len());

        let quote_arg = validation::escape_shell_arg("file'name");
        // Arguments with quotes should be escaped
        assert!(quote_arg.len() >= "file'name".len());
    }

    #[test]
    fn test_security_constants_are_reasonable() {
        // Validate that security constants are set to reasonable values using runtime checks
        // These checks ensure constants maintain sensible values during development

        // Check upload size is within reasonable bounds
        let upload_size = validation::MAX_UPLOAD_SIZE;
        let min_size = 1024 * 1024; // 1MB
        let max_size = 1024 * 1024 * 1024; // 1GB

        if upload_size <= min_size {
            panic!(
                "MAX_UPLOAD_SIZE ({}) must be greater than {} bytes (1MB)",
                upload_size, min_size
            );
        }

        if upload_size >= max_size {
            panic!(
                "MAX_UPLOAD_SIZE ({}) must be less than {} bytes (1GB)",
                upload_size, max_size
            );
        }

        // Check base64 size relationships
        let encoded_size = validation::MAX_BASE64_ENCODED_SIZE;
        let decoded_size = validation::MAX_BASE64_DECODED_SIZE;
        let upload_size_usize: usize = upload_size.try_into().unwrap();

        if encoded_size < upload_size_usize {
            panic!(
                "MAX_BASE64_ENCODED_SIZE ({}) must be >= MAX_UPLOAD_SIZE ({})",
                encoded_size, upload_size_usize
            );
        }

        if decoded_size > upload_size_usize {
            panic!(
                "MAX_BASE64_DECODED_SIZE ({}) must be <= MAX_UPLOAD_SIZE ({})",
                decoded_size, upload_size_usize
            );
        }

        // Check path depth limits
        let path_depth = validation::MAX_PATH_DEPTH;
        if path_depth == 0 {
            panic!("MAX_PATH_DEPTH must be greater than 0, got {}", path_depth);
        }

        if path_depth >= 100 {
            panic!(
                "MAX_PATH_DEPTH ({}) must be less than 100 for reasonable limits",
                path_depth
            );
        }
    }
}
