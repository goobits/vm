//! # Package Registry Server
//!
//! A multi-package registry server that supports PyPI, npm, and Cargo registries.
//! This library provides a unified interface for managing package repositories with
//! caching, upload, and download capabilities.
//!
//! ## Features
//!
//! - **Multi-format support**: Handles PyPI, npm, and Cargo package formats
//! - **Upstream caching**: Automatically caches packages from upstream registries
//! - **Package uploads**: Supports direct package uploads to the registry
//! - **Web UI**: Provides a web interface for package management
//! - **Security**: Configurable authentication and validation
//!
//! ## Key Modules
//!
//! - [`config`]: Configuration management and settings
//! - [`state`]: Application state and shared resources
//! - [`error`]: Error handling and standardized responses
//! - [`upstream`]: Communication with upstream registries
//! - [`api`]: HTTP API endpoints and routing
//! - [`storage`]: Package storage and file management
//! - [`validation`]: Security-focused input validation utilities
//!
//! ## Usage
//!
//! The main entry point is typically through the application server, but this library
//! exposes utilities for package name normalization, hashing, and filename validation
//! that can be used independently.

// Module declarations
pub mod api;
pub mod auth;
pub mod cargo;
#[cfg(not(test))]
pub mod client_ops;
pub mod config;
pub mod deletion;
pub mod error;
pub mod live_reload;
pub mod local_storage;
pub mod npm;
pub mod package_utils;
pub mod pypi;
pub mod server;
pub mod state;
pub mod storage;
pub mod types;
pub mod ui;
pub mod upstream;
pub mod utils;
pub mod validation;
pub mod validation_utils;

// Simplified configuration for VM tool integration
pub mod simple_config;

// NEW module declarations
pub mod hash_utils;
pub mod pypi_utils;

// Re-export key types for convenience
#[cfg(not(test))]
pub use client_ops::{add_package, list_packages, remove_package, show_status};
pub use config::Config;
pub use error::{ApiErrorResponse, AppError, AppResult, ErrorCode};
pub use server::{run_server, run_server_background, run_server_with_shutdown};
pub use state::{AppState, SuccessResponse};
pub use upstream::{UpstreamClient, UpstreamConfig};
pub use validation::{
    escape_shell_arg, sanitize_docker_name, validate_file_size, validate_hostname,
    validate_package_name, validate_safe_path, validate_version, ValidationError, ValidationResult,
    MAX_DESCRIPTION_LENGTH, MAX_FILENAME_LENGTH, MAX_PACKAGE_NAME_LENGTH, MAX_PATH_DEPTH,
    MAX_UPLOAD_SIZE, MAX_VERSION_LENGTH,
};

// NEW: Re-export utility functions from dedicated modules
pub use hash_utils::{sha1_hash, sha256_hash};
pub use pypi_utils::normalize_pypi_name;
pub use validation_utils::validate_filename; // Moved from lib.rs

/// Test utilities for common test patterns across modules
#[cfg(test)]
pub mod test_utils {
    use super::{AppState, UpstreamClient};
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Create test state with required directory structure for npm
    pub fn create_npm_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("npm/tarballs"))
            .expect("Failed to create npm tarballs dir");
        std::fs::create_dir_all(data_dir.join("npm/metadata"))
            .expect("Failed to create npm metadata dir");

        // Use disabled client to avoid TLS/Keychain prompts
        let upstream_client = Arc::new(UpstreamClient::disabled());
        let config = Arc::new(crate::config::Config::default());

        let state = Arc::new(AppState {
            data_dir,
            server_addr: "http://localhost:3080".to_string(),
            upstream_client,
            config,
        });
        (state, temp_dir)
    }

    /// Create test state with required directory structure for PyPI
    pub fn create_pypi_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("pypi/packages"))
            .expect("Failed to create pypi packages dir");

        // Use disabled client to avoid TLS/Keychain prompts
        let upstream_client = Arc::new(UpstreamClient::disabled());
        let config = Arc::new(crate::config::Config::default());

        let state = Arc::new(AppState {
            data_dir,
            server_addr: "http://localhost:3080".to_string(),
            upstream_client,
            config,
        });
        (state, temp_dir)
    }

    /// Create test state with required directory structure for Cargo
    pub fn create_cargo_test_state() -> (Arc<AppState>, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir for test");
        let data_dir = temp_dir.path().to_path_buf();

        // Create required directories
        std::fs::create_dir_all(data_dir.join("cargo/crates")).expect("Failed to create cargo crates dir");
        std::fs::create_dir_all(data_dir.join("cargo/index")).expect("Failed to create cargo index dir");

        // Use disabled client to avoid TLS/Keychain prompts
        let upstream_client = Arc::new(UpstreamClient::disabled());
        let config = Arc::new(crate::config::Config::default());

        let state = Arc::new(AppState {
            data_dir,
            server_addr: "http://localhost:3080".to_string(),
            upstream_client,
            config,
        });
        (state, temp_dir)
    }

    /// Create multipart body for testing uploads (used by PyPI tests)
    pub fn create_multipart_body(filename: &str, content: &[u8]) -> Vec<u8> {
        let boundary = "----formdata-axum-test";
        let mut body = Vec::new();

        body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"content\"; filename=\"");
        body.extend_from_slice(filename.as_bytes());
        body.extend_from_slice(b"\"\r\n");
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{}--\r\n", boundary).as_bytes());

        body
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;

    /// **PRIORITY 1 - Critical Security Tests for Server Address Validation**
    /// Tests for server address validation functions (imported from main.rs logic)
    mod server_address_validation_tests {
        // Note: These would test the functions from main.rs if they were public
        // For now, we'll create test versions of the validation logic

        fn test_validate_server_address(server_ip: &str) -> bool {
            use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

            // Check basic length constraints
            if server_ip.is_empty() || server_ip.len() > 253 {
                return false;
            }

            // Check for null bytes or control characters
            if server_ip.contains('\0') || server_ip.chars().any(|c| c.is_control()) {
                return false;
            }

            // Try to parse as IP address first
            if server_ip.parse::<IpAddr>().is_ok() {
                return true;
            }

            // Try to parse IPv4 specifically
            if server_ip.parse::<Ipv4Addr>().is_ok() {
                return true;
            }

            // Try to parse IPv6 specifically (handles bracketed format)
            let ipv6_candidate = if server_ip.starts_with('[') && server_ip.ends_with(']') {
                &server_ip[1..server_ip.len() - 1]
            } else {
                server_ip
            };
            if ipv6_candidate.parse::<Ipv6Addr>().is_ok() {
                return true;
            }

            // Check if it looks like an IPv4 address (all labels are numeric)
            let labels: Vec<&str> = server_ip.split('.').collect();
            if labels.len() >= 2
                && labels
                    .iter()
                    .all(|label| !label.is_empty() && label.chars().all(|c| c.is_ascii_digit()))
            {
                // Looks like IPv4-style (all numeric labels), but we already know it's invalid (parsing failed above)
                return false;
            }

            // Validate as hostname (DNS name)
            test_validate_hostname(server_ip)
        }

        fn test_validate_hostname(hostname: &str) -> bool {
            // Basic length check (DNS hostname max is 253 characters)
            if hostname.is_empty() || hostname.len() > 253 {
                return false;
            }

            // Cannot start or end with a dot
            if hostname.starts_with('.') || hostname.ends_with('.') {
                return false;
            }

            // Split into labels and validate each
            let labels: Vec<&str> = hostname.split('.').collect();
            if labels.is_empty() {
                return false;
            }

            for label in labels {
                // Each label must be 1-63 characters
                if label.is_empty() || label.len() > 63 {
                    return false;
                }

                // Cannot start or end with hyphen
                if label.starts_with('-') || label.ends_with('-') {
                    return false;
                }

                // Must contain only alphanumeric characters and hyphens
                if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                    return false;
                }
            }

            true
        }

        #[test]
        fn test_valid_ipv4_addresses() {
            let valid_ipv4_addresses = [
                "127.0.0.1",
                "192.168.1.1",
                "10.0.0.1",
                "172.16.0.1",
                "203.0.113.1",
                "8.8.8.8",
                "255.255.255.255",
                "0.0.0.0",
            ];

            for addr in &valid_ipv4_addresses {
                assert!(
                    test_validate_server_address(addr),
                    "Valid IPv4 address '{}' should pass validation",
                    addr
                );
            }
        }

        #[test]
        fn test_valid_ipv6_addresses() {
            let valid_ipv6_addresses = [
                "::1",
                "2001:db8::1",
                "fe80::1",
                "::ffff:127.0.0.1",
                "[::1]",
                "[2001:db8::1]",
                "2001:0db8:85a3:0000:0000:8a2e:0370:7334",
                "2001:db8:85a3::8a2e:370:7334",
            ];

            for addr in &valid_ipv6_addresses {
                assert!(
                    test_validate_server_address(addr),
                    "Valid IPv6 address '{}' should pass validation",
                    addr
                );
            }
        }

        #[test]
        fn test_valid_hostnames() {
            let valid_hostnames = [
                "localhost",
                "example.com",
                "sub.example.com",
                "my-server.local",
                "server123.example.org",
                "a.b.c.d.e.f.g.h.i.j",
                "test-123.example-site.com",
                "x", // single character hostname
            ];

            for hostname in &valid_hostnames {
                assert!(
                    test_validate_server_address(hostname),
                    "Valid hostname '{}' should pass validation",
                    hostname
                );
            }
        }

        #[test]
        fn test_invalid_ip_addresses_blocked() {
            let invalid_ips = [
                "256.256.256.256", // Invalid IPv4
                "192.168.1",       // Incomplete IPv4
                "192.168.1.1.1",   // Too many octets
                "192.168.1.256",   // Octet too large
                "2001:db8::1::2",  // Invalid IPv6 (double ::)
                "gggg::1",         // Invalid hex characters
                "192.168.1.-1",    // Negative number
                "192.168.1.01",    // Leading zero (technically invalid)
            ];

            for addr in &invalid_ips {
                assert!(
                    !test_validate_server_address(addr),
                    "Invalid IP address '{}' should be rejected",
                    addr
                );
            }
        }

        #[test]
        fn test_injection_attacks_blocked() {
            let injection_attacks = [
                "127.0.0.1; rm -rf /",
                "127.0.0.1`whoami`",
                "127.0.0.1$(whoami)",
                "127.0.0.1 && rm -rf /",
                "127.0.0.1|nc -l 4444",
                "example.com; echo 'pwned'",
                "host\nmalicious-command",
                "host\rmalicious-command",
                "host\tmalicious-command",
                "host\0malicious-command",
            ];

            for attack in &injection_attacks {
                assert!(
                    !test_validate_server_address(attack),
                    "Injection attack '{}' should be blocked",
                    attack
                );
            }
        }

        #[test]
        fn test_hostname_edge_cases_blocked() {
            let invalid_hostnames = [
                "",                                 // Empty
                ".example.com",                     // Starts with dot
                "example.com.",                     // Ends with dot
                "ex..ample.com",                    // Double dots
                "-example.com",                     // Starts with hyphen
                "example-.com",                     // Ends with hyphen
                "example.com-",                     // Label ends with hyphen
                &"a".repeat(254),                   // Too long (254 chars)
                &format!("{}.com", "a".repeat(64)), // Label too long (64 chars)
                "exam ple.com",                     // Contains space
                "example..com",                     // Empty label
                "example.com!",                     // Invalid character
                "example.com@",                     // Invalid character
                "exam#ple.com",                     // Invalid character
            ];

            for hostname in &invalid_hostnames {
                assert!(
                    !test_validate_server_address(hostname),
                    "Invalid hostname '{}' should be rejected",
                    hostname
                );
            }
        }

        #[test]
        fn test_control_character_injection_blocked() {
            let control_char_attacks = [
                "127.0.0.1\x01",
                "127.0.0.1\x02",
                "127.0.0.1\x03",
                "example.com\x08",
                "example.com\x0c",
                "example.com\x7f",
                "\x1fexample.com",
                "exam\x1fple.com",
            ];

            for attack in &control_char_attacks {
                assert!(
                    !test_validate_server_address(attack),
                    "Control character injection '{}' should be blocked",
                    attack.escape_debug()
                );
            }
        }

        #[test]
        fn test_null_byte_injection_blocked() {
            let null_byte_attacks = [
                "127.0.0.1\0",
                "\0example.com",
                "exam\0ple.com",
                "example.com\0.evil.com",
                "127.0.0.1\0; rm -rf /",
            ];

            for attack in &null_byte_attacks {
                assert!(
                    !test_validate_server_address(attack),
                    "Null byte injection should be blocked: {:?}",
                    attack
                );
            }
        }

        #[test]
        fn test_hostname_length_limits() {
            // Test hostname length limits (253 chars total with multiple labels)
            // Create a hostname that's exactly 253 characters: 63 + 1 + 63 + 1 + 63 + 1 + 61 = 253
            let max_valid_hostname = format!(
                "{}.{}.{}.{}",
                "a".repeat(63),
                "a".repeat(63),
                "a".repeat(63),
                "a".repeat(61)
            );
            assert_eq!(max_valid_hostname.len(), 253);
            assert!(
                test_validate_server_address(&max_valid_hostname),
                "Hostname with 253 characters should be valid"
            );

            let too_long_hostname = "a".repeat(254);
            assert!(
                !test_validate_server_address(&too_long_hostname),
                "Hostname with 254 characters should be invalid"
            );

            // Test label length limits
            let max_valid_label = format!("{}.com", "a".repeat(63));
            assert!(
                test_validate_server_address(&max_valid_label),
                "Hostname with 63-character label should be valid"
            );

            let too_long_label = format!("{}.com", "a".repeat(64));
            assert!(
                !test_validate_server_address(&too_long_label),
                "Hostname with 64-character label should be invalid"
            );
        }
    }

    /// **PRIORITY 2 - Error Handling Tests for Storage Operations**
    /// Tests for storage module error handling and edge cases
    mod storage_error_handling_tests {
        use super::*;
        use crate::storage;
        use tempfile::TempDir;

        #[tokio::test]
        async fn test_read_nonexistent_file() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let nonexistent_path = temp_dir.path().join("nonexistent.txt");

            let result = storage::read_file(&nonexistent_path).await;
            assert!(
                result.is_err(),
                "Reading nonexistent file should return error"
            );

            if let Err(AppError::NotFound(msg)) = result {
                assert!(
                    msg.contains("File not found"),
                    "Error message should mention file not found"
                );
                assert!(
                    msg.contains(&nonexistent_path.display().to_string()),
                    "Error message should include the file path"
                );
            } else {
                panic!("Expected NotFound error, got: {:?}", result);
            }
        }

        #[tokio::test]
        async fn test_read_file_string_nonexistent() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let nonexistent_path = temp_dir.path().join("nonexistent.txt");

            let result = storage::read_file_string(&nonexistent_path).await;
            assert!(
                result.is_err(),
                "Reading nonexistent file string should return error"
            );

            if let Err(AppError::NotFound(msg)) = result {
                assert!(
                    msg.contains("File not found"),
                    "Error message should mention file not found"
                );
            } else {
                panic!("Expected NotFound error, got: {:?}", result);
            }
        }

        #[tokio::test]
        async fn test_save_file_creates_parent_directories() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let nested_path = temp_dir
                .path()
                .join("deeply")
                .join("nested")
                .join("directory")
                .join("file.txt");

            let test_content = b"test content for nested directory";
            let result = storage::save_file(&nested_path, test_content).await;
            assert!(
                result.is_ok(),
                "Saving file should create parent directories"
            );

            // Verify the file was created and has correct content
            assert!(nested_path.exists(), "File should exist after saving");
            let saved_content = std::fs::read(&nested_path).expect("Failed to read saved file");
            assert_eq!(
                saved_content, test_content,
                "Saved content should match original"
            );
        }

        #[tokio::test]
        async fn test_append_to_nonexistent_file() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let new_file_path = temp_dir.path().join("new_file.txt");

            let content = b"first line content";
            let result = storage::append_to_file(&new_file_path, content).await;
            assert!(
                result.is_ok(),
                "Appending to nonexistent file should create it"
            );

            // Verify the file was created with correct content
            assert!(
                new_file_path.exists(),
                "File should be created when appending"
            );
            let file_content = std::fs::read_to_string(&new_file_path).expect("Failed to read file");
            assert!(
                file_content.contains("first line content"),
                "File should contain appended content"
            );
            assert!(
                file_content.ends_with('\n'),
                "File should end with newline after append"
            );
        }

        #[tokio::test]
        async fn test_append_to_existing_file() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let file_path = temp_dir.path().join("existing_file.txt");

            // Create initial file content
            std::fs::write(&file_path, "initial content").expect("Failed to write initial file");

            let additional_content = b"appended content";
            let result = storage::append_to_file(&file_path, additional_content).await;
            assert!(result.is_ok(), "Appending to existing file should succeed");

            // Verify both contents are present
            let file_content = std::fs::read_to_string(&file_path).expect("Failed to read file");
            assert!(
                file_content.contains("initial content"),
                "File should retain original content"
            );
            assert!(
                file_content.contains("appended content"),
                "File should contain appended content"
            );
        }

        #[tokio::test]
        async fn test_append_with_invalid_utf8() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let file_path = temp_dir.path().join("utf8_test.txt");

            // Try to append invalid UTF-8 bytes
            let invalid_utf8 = vec![0xFF, 0xFE, 0xFD]; // Invalid UTF-8 sequence
            let result = storage::append_to_file(&file_path, &invalid_utf8).await;

            assert!(
                result.is_err(),
                "Appending invalid UTF-8 should return error"
            );
            if let Err(AppError::Utf8(_)) = result {
                // Expected UTF-8 error
            } else {
                panic!("Expected UTF-8 error, got: {:?}", result);
            }
        }

        #[tokio::test]
        async fn test_save_empty_file() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let empty_file_path = temp_dir.path().join("empty.txt");

            let result = storage::save_file(&empty_file_path, b"").await;
            assert!(result.is_ok(), "Saving empty file should succeed");

            assert!(empty_file_path.exists(), "Empty file should be created");
            let content = std::fs::read(&empty_file_path).expect("Failed to read empty file");
            assert!(content.is_empty(), "File should be empty");
        }

        #[tokio::test]
        async fn test_read_empty_file() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let empty_file_path = temp_dir.path().join("empty.txt");

            // Create empty file
            std::fs::write(&empty_file_path, b"").expect("Failed to write empty file");

            let result = storage::read_file(&empty_file_path).await;
            assert!(result.is_ok(), "Reading empty file should succeed");

            let content = result.expect("Should be able to read empty file");
            assert!(content.is_empty(), "Empty file should return empty content");
        }

        #[tokio::test]
        async fn test_append_maintains_newlines() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let file_path = temp_dir.path().join("newline_test.txt");

            // Create initial file without trailing newline
            std::fs::write(&file_path, "line without newline").expect("Failed to write initial file");

            // Append content
            let result = storage::append_to_file(&file_path, b"new line").await;
            assert!(result.is_ok(), "Appending should succeed");

            let content = std::fs::read_to_string(&file_path).expect("Failed to read file");
            let lines: Vec<&str> = content.lines().collect();
            assert_eq!(lines.len(), 2, "Should have two lines after append");
            assert_eq!(lines[0], "line without newline");
            assert_eq!(lines[1], "new line");
        }

        #[tokio::test]
        async fn test_large_file_handling() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let large_file_path = temp_dir.path().join("large_file.txt");

            // Create a moderately large content (1MB)
            let large_content = vec![b'A'; 1024 * 1024];
            let result = storage::save_file(&large_file_path, &large_content).await;
            assert!(result.is_ok(), "Saving large file should succeed");

            // Read it back
            let read_result = storage::read_file(&large_file_path).await;
            assert!(read_result.is_ok(), "Reading large file should succeed");

            let read_content = read_result.expect("Should be able to read large file");
            assert_eq!(
                read_content.len(),
                large_content.len(),
                "Content size should match"
            );
            assert_eq!(read_content, large_content, "Content should match exactly");
        }

        #[tokio::test]
        async fn test_path_security_validation() {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");

            // Test various problematic paths - these should not cause panics or security issues
            let problematic_paths = [
                "../../../etc/passwd",
                "..\\..\\..\\windows\\system32\\config\\sam",
                "/etc/passwd",
                "C:\\windows\\system32\\cmd.exe",
                "file\0name",
                "file\x00name",
            ];

            for path_str in &problematic_paths {
                let problematic_path = temp_dir.path().join(path_str);

                // These operations should not panic or cause security issues
                // They may fail (which is fine), but should handle gracefully
                let save_result = storage::save_file(&problematic_path, b"test").await;
                let read_result = storage::read_file(&problematic_path).await;

                // We don't assert success/failure here, just that they don't panic
                // The actual security validation should happen at a higher level
                drop(save_result);
                drop(read_result);
            }
        }
    }
}
