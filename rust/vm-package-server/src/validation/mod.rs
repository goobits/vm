//! # Input Validation Utilities
//!
//! This module provides security-focused validation helpers for sanitizing and validating
//! various types of input data throughout the package server. All functions follow a
//! security-first approach to prevent injection attacks and ensure data integrity.
//!
//! ## Security Features
//!
//! - Shell command injection prevention through proper escaping
//! - Path traversal attack prevention with strict validation
//! - Docker parameter sanitization for secure container operations
//! - Size limits and bounds checking to prevent resource exhaustion
//!
//! ## Usage
//!
//! ```rust
//! // use vm_package_server::validation::{escape_shell_arg, validate_safe_path, sanitize_docker_name};
//!
//! // Safely escape shell arguments
//! // let safe_arg = escape_shell_arg("user input with spaces");
//! // assert_eq!(safe_arg, "'user input with spaces'");
//!
//! // Validate file paths
//! // if validate_safe_path("safe/relative/path").is_ok() {
//! //     // Path is safe to use
//! // }
//!
//! // Sanitize Docker container names
//! // let container_name = sanitize_docker_name("my-app-container").unwrap();
//! // assert_eq!(container_name, "my-app-container");
//! ```

pub mod docker;
pub mod error;
pub mod http;
pub mod limits;
pub mod manifests;
pub mod paths;
pub mod result;
pub mod shell;

pub use self::{
    docker::{
        sanitize_docker_name, validate_docker_image_name, validate_docker_port,
        validate_docker_volume_path,
    },
    error::ValidationError,
    http::{
        validate_base64_characters, validate_base64_size, validate_hostname,
        validate_multipart_limits,
    },
    limits::{
        validate_file_size, MAX_BASE64_DECODED_SIZE, MAX_BASE64_ENCODED_SIZE,
        MAX_DESCRIPTION_LENGTH, MAX_FILENAME_LENGTH, MAX_METADATA_SIZE, MAX_MULTIPART_FIELDS,
        MAX_PACKAGE_FILE_SIZE, MAX_PACKAGE_NAME_LENGTH, MAX_PATH_DEPTH, MAX_REQUEST_BODY_SIZE,
        MAX_UPLOAD_SIZE, MAX_VERSION_LENGTH, MEMORY_THRESHOLD,
    },
    manifests::{validate_cargo_upload_structure, validate_package_name, validate_version},
    paths::validate_safe_path,
    result::ValidationResult,
    shell::escape_shell_arg,
};
