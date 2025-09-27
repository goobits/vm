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
//! use crate::validation::{escape_shell_arg, validate_safe_path, sanitize_docker_name};
//!
//! // Safely escape shell arguments
//! let safe_arg = escape_shell_arg("user input with spaces");
//! assert_eq!(safe_arg, "'user input with spaces'");
//!
//! // Validate file paths
//! if validate_safe_path("safe/relative/path").is_ok() {
//!     // Path is safe to use
//! }
//!
//! // Sanitize Docker container names
//! let container_name = sanitize_docker_name("my-app-container").unwrap();
//! assert_eq!(container_name, "my-app-container");
//! ```

use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

/// Maximum allowed file size for uploads (100 MB)
pub const MAX_UPLOAD_SIZE: u64 = 100 * 1024 * 1024;

/// Maximum allowed request body size (120 MB) - allows overhead for multipart encoding
pub const MAX_REQUEST_BODY_SIZE: usize = 120 * 1024 * 1024;

/// Maximum allowed package file size (80 MB) - actual package content limit
pub const MAX_PACKAGE_FILE_SIZE: u64 = 80 * 1024 * 1024;

/// Maximum allowed base64 encoded size (110 MB) - to prevent base64 bombs
pub const MAX_BASE64_ENCODED_SIZE: usize = 110 * 1024 * 1024;

/// Maximum allowed decoded base64 size (80 MB) - actual data after decoding
pub const MAX_BASE64_DECODED_SIZE: usize = 80 * 1024 * 1024;

/// Maximum allowed metadata size for package manifests (1 MB)
#[allow(dead_code)]
pub const MAX_METADATA_SIZE: usize = 1024 * 1024;

/// Maximum allowed number of multipart fields
pub const MAX_MULTIPART_FIELDS: usize = 10;

/// Memory threshold for streaming vs loading into memory (10 MB)
#[allow(dead_code)]
pub const MEMORY_THRESHOLD: usize = 10 * 1024 * 1024;

/// Maximum allowed package name length
#[allow(dead_code)]
pub const MAX_PACKAGE_NAME_LENGTH: usize = 214;

/// Maximum allowed version string length
#[allow(dead_code)]
pub const MAX_VERSION_LENGTH: usize = 64;

/// Maximum allowed description length
#[allow(dead_code)]
pub const MAX_DESCRIPTION_LENGTH: usize = 4096;

/// Maximum allowed filename length
#[allow(dead_code)]
pub const MAX_FILENAME_LENGTH: usize = 255;

/// Maximum allowed path depth for relative paths
#[allow(dead_code)]
pub const MAX_PATH_DEPTH: usize = 10;

/// Regex for validating Docker container/image names
#[allow(dead_code)]
static DOCKER_NAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9._-]*$")
        .expect("Docker name regex should compile - this is a static pattern")
});

/// Regex for validating hostnames (RFC 1123 compliant)
static HOSTNAME_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$")
        .expect("Hostname regex should compile - this is a static RFC 1123 pattern")
});

/// Error types for validation failures
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Input too long: {actual} exceeds maximum {max}")]
    TooLong { actual: usize, max: usize },

    #[error("Input too short: {actual} is below minimum {min}")]
    TooShort { actual: usize, min: usize },

    #[error("Invalid characters in input: {input}")]
    InvalidCharacters { input: String },

    #[error("Path traversal detected: {path}")]
    #[allow(dead_code)]
    PathTraversal { path: String },

    #[error("Absolute path not allowed: {path}")]
    #[allow(dead_code)]
    AbsolutePath { path: String },

    #[error("Path depth exceeds maximum: {actual} > {max}")]
    #[allow(dead_code)]
    PathTooDeep { actual: usize, max: usize },

    #[error("File size exceeds limit: {actual} > {max}")]
    FileTooLarge { actual: u64, max: u64 },

    #[error("Invalid format: {reason}")]
    InvalidFormat { reason: String },

    #[error("Contains null bytes")]
    NullBytes,

    #[error("Contains control characters")]
    ControlCharacters,
}

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Escape a shell argument to prevent command injection attacks.
///
/// This function properly escapes shell metacharacters and quotes the argument
/// to ensure it can be safely passed to shell commands. It uses single quotes
/// for maximum safety and handles embedded single quotes correctly.
///
/// # Arguments
///
/// * `arg` - The argument to escape
///
/// # Returns
///
/// A safely escaped string that can be used in shell commands
///
/// # Examples
///
/// ```rust
/// use crate::validation::escape_shell_arg;
///
/// let safe = escape_shell_arg("file with spaces.txt");
/// assert_eq!(safe, "'file with spaces.txt'");
///
/// let safe = escape_shell_arg("file'with'quotes.txt");
/// assert_eq!(safe, "'file'\\''with'\\''quotes.txt'");
/// ```
pub fn escape_shell_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }

    // Check for null bytes
    if arg.contains('\0') {
        // Return empty quoted string for safety
        return "''".to_string();
    }

    // If the argument contains only safe characters, return it as-is
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
    {
        return arg.to_string();
    }

    // Otherwise, wrap in single quotes and escape any embedded single quotes
    format!("'{}'", arg.replace('\'', "'\\''"))
}

/// Validate that a path is safe from directory traversal attacks.
///
/// This function checks for path traversal attempts, ensures the path is relative,
/// and validates that it doesn't exceed maximum depth limits. It also checks for
/// dangerous characters and patterns.
///
/// # Arguments
///
/// * `path` - The path to validate
///
/// # Returns
///
/// `Ok(PathBuf)` if the path is safe, `Err(ValidationError)` otherwise
///
/// # Examples
///
/// ```rust
/// use crate::validation::validate_safe_path;
///
/// // Safe relative path
/// assert!(validate_safe_path("packages/mypackage/1.0.0").is_ok());
///
/// // Path traversal attempt
/// assert!(validate_safe_path("../../../etc/passwd").is_err());
///
/// // Absolute path
/// assert!(validate_safe_path("/etc/passwd").is_err());
/// ```
#[allow(dead_code)]
pub fn validate_safe_path<P: AsRef<Path>>(path: P) -> ValidationResult<PathBuf> {
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

    // Reject absolute paths
    if path.is_absolute() {
        return Err(ValidationError::AbsolutePath {
            path: path_str.to_string(),
        });
    }

    // Check for path traversal patterns
    if path_str.contains("..") {
        return Err(ValidationError::PathTraversal {
            path: path_str.to_string(),
        });
    }

    // Check path depth
    let depth = path.components().count();
    if depth > MAX_PATH_DEPTH {
        return Err(ValidationError::PathTooDeep {
            actual: depth,
            max: MAX_PATH_DEPTH,
        });
    }

    // Additional checks for dangerous patterns
    let dangerous_patterns = [
        "//", "\\\\", "~", "$", "`", "|", "&", ";", "<", ">", "(", ")", "{", "}", "[", "]", "*",
        "?",
    ];

    for pattern in &dangerous_patterns {
        if path_str.contains(pattern) {
            return Err(ValidationError::InvalidCharacters {
                input: path_str.to_string(),
            });
        }
    }

    Ok(path.to_path_buf())
}

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
/// use crate::validation::sanitize_docker_name;
///
/// // Valid Docker name
/// assert!(sanitize_docker_name("my-app-container").is_ok());
///
/// // Invalid characters
/// assert!(sanitize_docker_name("my app container").is_err());
/// ```
#[allow(dead_code)]
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

/// Validate package names according to ecosystem-specific rules.
///
/// This function validates package names for npm, PyPI, and Cargo ecosystems,
/// ensuring they conform to the respective naming conventions and don't contain
/// malicious characters.
///
/// # Arguments
///
/// * `name` - The package name to validate
/// * `ecosystem` - The package ecosystem ("npm", "pypi", "cargo")
///
/// # Returns
///
/// `Ok(String)` with the validated name, `Err(ValidationError)` if invalid
#[allow(dead_code)]
pub fn validate_package_name(name: &str, ecosystem: &str) -> ValidationResult<String> {
    // Common validations
    if name.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if name.len() > MAX_PACKAGE_NAME_LENGTH {
        return Err(ValidationError::TooLong {
            actual: name.len(),
            max: MAX_PACKAGE_NAME_LENGTH,
        });
    }

    // Check for null bytes and control characters
    if name.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    if name.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Basic character validation - allow letters, numbers, dots, hyphens, underscores
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    // Ecosystem-specific validations
    match ecosystem.to_lowercase().as_str() {
        "npm" => validate_npm_package_name(name),
        "pypi" => validate_pypi_package_name(name),
        "cargo" => validate_cargo_package_name(name),
        _ => Err(ValidationError::InvalidFormat {
            reason: format!("Unknown ecosystem: {}", ecosystem),
        }),
    }
}

/// Validate npm package names according to npm rules
#[allow(dead_code)]
fn validate_npm_package_name(name: &str) -> ValidationResult<String> {
    // npm specific rules
    if name.starts_with('.') || name.starts_with('_') {
        return Err(ValidationError::InvalidFormat {
            reason: "npm package names cannot start with . or _".to_string(),
        });
    }

    if name.to_lowercase() != name {
        return Err(ValidationError::InvalidFormat {
            reason: "npm package names must be lowercase".to_string(),
        });
    }

    // Check for URL-unsafe characters
    if name
        .chars()
        .any(|c| matches!(c, ' ' | '/' | '%' | '&' | '?' | '#'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    Ok(name.to_string())
}

/// Validate PyPI package names according to PEP 508
#[allow(dead_code)]
fn validate_pypi_package_name(name: &str) -> ValidationResult<String> {
    // PyPI allows letters, numbers, hyphens, periods, and underscores
    // Names are case-insensitive but normalized to lowercase with hyphens
    if name
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '.' | '_'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    // Cannot start with numbers
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return Err(ValidationError::InvalidFormat {
            reason: "PyPI package names cannot start with numbers".to_string(),
        });
    }

    Ok(name.to_string())
}

/// Validate Cargo package names according to Cargo rules
#[allow(dead_code)]
fn validate_cargo_package_name(name: &str) -> ValidationResult<String> {
    // Cargo allows letters, numbers, hyphens, and underscores
    if name
        .chars()
        .any(|c| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: name.to_string(),
        });
    }

    // Cannot be empty or start with numbers
    if name.chars().next().is_none_or(|c| c.is_ascii_digit()) {
        return Err(ValidationError::InvalidFormat {
            reason: "Cargo package names cannot start with numbers".to_string(),
        });
    }

    Ok(name.to_string())
}

/// Validate version strings according to semantic versioning principles.
///
/// This function validates version strings to ensure they follow a reasonable
/// format and don't contain malicious characters.
///
/// # Arguments
///
/// * `version` - The version string to validate
///
/// # Returns
///
/// `Ok(String)` with the validated version, `Err(ValidationError)` if invalid
#[allow(dead_code)]
pub fn validate_version(version: &str) -> ValidationResult<String> {
    if version.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if version.len() > MAX_VERSION_LENGTH {
        return Err(ValidationError::TooLong {
            actual: version.len(),
            max: MAX_VERSION_LENGTH,
        });
    }

    // Check for null bytes and control characters
    if version.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    if version.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Validate version format - allow letters, numbers, dots, hyphens, underscores, plus
    if !version
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
    {
        return Err(ValidationError::InvalidCharacters {
            input: version.to_string(),
        });
    }

    Ok(version.to_string())
}

/// Validate file size against limits.
///
/// This function checks if a file size is within acceptable limits to prevent
/// resource exhaustion attacks.
///
/// # Arguments
///
/// * `size` - The file size in bytes
/// * `max_size` - Optional custom maximum size (defaults to MAX_UPLOAD_SIZE)
///
/// # Returns
///
/// `Ok(())` if size is acceptable, `Err(ValidationError)` if too large
#[allow(dead_code)]
pub fn validate_file_size(size: u64, max_size: Option<u64>) -> ValidationResult<()> {
    let limit = max_size.unwrap_or(MAX_UPLOAD_SIZE);

    if size > limit {
        return Err(ValidationError::FileTooLarge {
            actual: size,
            max: limit,
        });
    }

    Ok(())
}

/// Validate and sanitize hostnames for network operations.
///
/// This function validates hostnames according to RFC 1123 to ensure they
/// are safe for network operations and don't contain injection patterns.
///
/// # Arguments
///
/// * `hostname` - The hostname to validate
///
/// # Returns
///
/// `Ok(String)` with the validated hostname, `Err(ValidationError)` if invalid
pub fn validate_hostname(hostname: &str) -> ValidationResult<String> {
    if hostname.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    if hostname.len() > 253 {
        return Err(ValidationError::TooLong {
            actual: hostname.len(),
            max: 253,
        });
    }

    // Check for null bytes and control characters
    if hostname.contains('\0') {
        return Err(ValidationError::NullBytes);
    }

    if hostname.chars().any(|c| c.is_control()) {
        return Err(ValidationError::ControlCharacters);
    }

    // Validate hostname format
    if !HOSTNAME_REGEX.is_match(hostname) {
        return Err(ValidationError::InvalidCharacters {
            input: hostname.to_string(),
        });
    }

    // Additional checks
    if hostname.starts_with('-') || hostname.ends_with('-') {
        return Err(ValidationError::InvalidFormat {
            reason: "Hostnames cannot start or end with hyphens".to_string(),
        });
    }

    if hostname.starts_with('.') || hostname.ends_with('.') {
        return Err(ValidationError::InvalidFormat {
            reason: "Hostnames cannot start or end with dots".to_string(),
        });
    }

    Ok(hostname.to_string())
}

/// Validate base64 encoded data size before decoding to prevent base64 bombs.
///
/// This function checks the size of base64 encoded data before attempting to decode
/// it, preventing memory exhaustion attacks from maliciously crafted base64 data.
///
/// # Arguments
///
/// * `encoded_data` - The base64 encoded string to validate
/// * `max_encoded_size` - Optional maximum encoded size (defaults to MAX_BASE64_ENCODED_SIZE)
/// * `max_decoded_size` - Optional maximum decoded size (defaults to MAX_BASE64_DECODED_SIZE)
///
/// # Returns
///
/// `Ok(())` if the encoded data is within limits, `Err(ValidationError)` otherwise
pub fn validate_base64_size(
    encoded_data: &str,
    max_encoded_size: Option<usize>,
    max_decoded_size: Option<usize>,
) -> ValidationResult<()> {
    let encoded_limit = max_encoded_size.unwrap_or(MAX_BASE64_ENCODED_SIZE);
    let decoded_limit = max_decoded_size.unwrap_or(MAX_BASE64_DECODED_SIZE);

    // Check encoded size first
    if encoded_data.len() > encoded_limit {
        return Err(ValidationError::FileTooLarge {
            actual: encoded_data.len() as u64,
            max: encoded_limit as u64,
        });
    }

    // Estimate decoded size (base64 encodes 3 bytes as 4 characters)
    let estimated_decoded_size = (encoded_data.len() * 3) / 4;
    if estimated_decoded_size > decoded_limit {
        return Err(ValidationError::FileTooLarge {
            actual: estimated_decoded_size as u64,
            max: decoded_limit as u64,
        });
    }

    Ok(())
}

/// Validate that base64 data contains only valid characters.
///
/// This function ensures base64 data contains only valid base64 characters
/// to prevent injection attacks and invalid data processing.
///
/// # Arguments
///
/// * `data` - The base64 string to validate
///
/// # Returns
///
/// `Ok(())` if valid base64 characters, `Err(ValidationError)` otherwise
pub fn validate_base64_characters(data: &str) -> ValidationResult<()> {
    if data.is_empty() {
        return Err(ValidationError::TooShort { actual: 0, min: 1 });
    }

    // Check for valid base64 characters only
    if !data
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
    {
        return Err(ValidationError::InvalidCharacters {
            input: "Invalid base64 characters detected".to_string(),
        });
    }

    Ok(())
}

/// Validate multipart upload limits to prevent memory exhaustion.
///
/// This function validates multipart upload parameters including field count,
/// field sizes, and total payload size to prevent DoS attacks.
///
/// # Arguments
///
/// * `field_count` - Number of multipart fields
/// * `total_size` - Total size of all fields combined
/// * `max_fields` - Optional maximum field count (defaults to MAX_MULTIPART_FIELDS)
///
/// # Returns
///
/// `Ok(())` if within limits, `Err(ValidationError)` otherwise
pub fn validate_multipart_limits(
    field_count: usize,
    total_size: u64,
    max_fields: Option<usize>,
) -> ValidationResult<()> {
    let field_limit = max_fields.unwrap_or(MAX_MULTIPART_FIELDS);

    if field_count > field_limit {
        return Err(ValidationError::TooLong {
            actual: field_count,
            max: field_limit,
        });
    }

    if total_size > MAX_UPLOAD_SIZE {
        return Err(ValidationError::FileTooLarge {
            actual: total_size,
            max: MAX_UPLOAD_SIZE,
        });
    }

    Ok(())
}

/// Validate cargo upload payload structure and size limits.
///
/// This function validates the binary structure of cargo upload payloads
/// to ensure they conform to expected format and size limits.
///
/// # Arguments
///
/// * `payload_size` - Total size of the upload payload
/// * `metadata_size` - Size of the metadata portion
/// * `crate_size` - Size of the crate file portion
///
/// # Returns
///
/// `Ok(())` if valid structure and sizes, `Err(ValidationError)` otherwise
#[allow(dead_code)]
pub fn validate_cargo_upload_structure(
    payload_size: usize,
    metadata_size: usize,
    crate_size: usize,
) -> ValidationResult<()> {
    // Validate total payload size
    if payload_size > MAX_REQUEST_BODY_SIZE {
        return Err(ValidationError::FileTooLarge {
            actual: payload_size as u64,
            max: MAX_REQUEST_BODY_SIZE as u64,
        });
    }

    // Validate metadata size
    if metadata_size > MAX_METADATA_SIZE {
        return Err(ValidationError::FileTooLarge {
            actual: metadata_size as u64,
            max: MAX_METADATA_SIZE as u64,
        });
    }

    // Validate crate file size
    if crate_size > MAX_PACKAGE_FILE_SIZE as usize {
        return Err(ValidationError::FileTooLarge {
            actual: crate_size as u64,
            max: MAX_PACKAGE_FILE_SIZE,
        });
    }

    // Validate structure: payload should be approximately metadata + crate + headers
    let expected_min_size = metadata_size + crate_size + 8; // 8 bytes for length headers
    if payload_size < expected_min_size {
        return Err(ValidationError::InvalidFormat {
            reason: format!(
                "Payload size {} is smaller than expected minimum {}",
                payload_size, expected_min_size
            ),
        });
    }

    Ok(())
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    fn test_escape_shell_arg() {
        assert_eq!(escape_shell_arg("simple"), "simple");
        assert_eq!(escape_shell_arg("file with spaces"), "'file with spaces'");
        assert_eq!(
            escape_shell_arg("file'with'quotes"),
            "'file'\\''with'\\''quotes'"
        );
        assert_eq!(escape_shell_arg(""), "''");
        assert_eq!(escape_shell_arg("file\0with\0nulls"), "''");
    }

    #[test]
    fn test_validate_safe_path() {
        // Valid paths
        assert!(validate_safe_path("packages/mypackage").is_ok());
        assert!(validate_safe_path("data/npm/tarballs/package.tgz").is_ok());

        // Invalid paths
        assert!(validate_safe_path("../../../etc/passwd").is_err());
        assert!(validate_safe_path("/etc/passwd").is_err());
        assert!(validate_safe_path("path/with/../traversal").is_err());
        assert!(validate_safe_path("path/with/null\0byte").is_err());
    }

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
    fn test_validate_package_name() {
        // npm tests
        assert!(validate_package_name("express", "npm").is_ok());
        assert!(validate_package_name("@scope/package", "npm").is_err()); // @ not in basic regex
        assert!(validate_package_name("Express", "npm").is_err()); // must be lowercase

        // PyPI tests
        assert!(validate_package_name("django", "pypi").is_ok());
        assert!(validate_package_name("django-rest-framework", "pypi").is_ok());
        assert!(validate_package_name("123invalid", "pypi").is_err()); // cannot start with number

        // Cargo tests
        assert!(validate_package_name("serde", "cargo").is_ok());
        assert!(validate_package_name("serde_json", "cargo").is_ok());
        assert!(validate_package_name("123invalid", "cargo").is_err()); // cannot start with number
    }

    #[test]
    fn test_validate_version() {
        assert!(validate_version("1.0.0").is_ok());
        assert!(validate_version("2.1.0-beta.1").is_ok());
        assert!(validate_version("").is_err());
        assert!(validate_version("version\0with\0nulls").is_err());

        // Test length limit
        let long_version = "1.0.0-".to_string() + &"a".repeat(100);
        assert!(validate_version(&long_version).is_err());
    }

    #[test]
    fn test_validate_file_size() {
        assert!(validate_file_size(1024, None).is_ok());
        assert!(validate_file_size(MAX_UPLOAD_SIZE, None).is_ok());
        assert!(validate_file_size(MAX_UPLOAD_SIZE + 1, None).is_err());
        assert!(validate_file_size(1024, Some(512)).is_err());
    }

    #[test]
    fn test_validate_hostname() {
        assert!(validate_hostname("example.com").is_ok());
        assert!(validate_hostname("sub.example.com").is_ok());
        assert!(validate_hostname("localhost").is_ok());

        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("-invalid.com").is_err());
        assert!(validate_hostname("invalid-.com").is_err());
        assert!(validate_hostname(".invalid.com").is_err());
        assert!(validate_hostname("invalid.com.").is_err());
    }

    #[test]
    fn test_validate_base64_size() {
        // Valid base64 data within limits
        let small_base64 = "SGVsbG8gV29ybGQ="; // "Hello World"
        assert!(validate_base64_size(small_base64, None, None).is_ok());

        // Test with custom limits
        assert!(validate_base64_size(small_base64, Some(100), Some(50)).is_ok());

        // Test encoded size limit
        let large_encoded = "a".repeat(MAX_BASE64_ENCODED_SIZE + 1);
        assert!(validate_base64_size(&large_encoded, None, None).is_err());

        // Test estimated decoded size limit (create string that will decode to more than the limit)
        // Base64 encodes 3 bytes as 4 characters, so we need 4/3 * limit + padding to exceed it
        let size_that_decodes_too_large = "a".repeat(((MAX_BASE64_DECODED_SIZE * 4) / 3) + 100);
        assert!(validate_base64_size(&size_that_decodes_too_large, None, None).is_err());
    }

    #[test]
    fn test_validate_base64_characters() {
        // Valid base64
        assert!(validate_base64_characters("SGVsbG8gV29ybGQ=").is_ok());
        assert!(validate_base64_characters("YWJjZGVmZw==").is_ok());

        // Invalid characters
        assert!(validate_base64_characters("Hello World!").is_err());
        assert!(validate_base64_characters("abc@def").is_err());
        assert!(validate_base64_characters("abc def").is_err());

        // Empty string
        assert!(validate_base64_characters("").is_err());
    }

    #[test]
    fn test_validate_multipart_limits() {
        // Valid limits
        assert!(validate_multipart_limits(5, 1024 * 1024, None).is_ok());

        // Too many fields
        assert!(validate_multipart_limits(MAX_MULTIPART_FIELDS + 1, 1024, None).is_err());

        // Total size too large
        assert!(validate_multipart_limits(5, MAX_UPLOAD_SIZE + 1, None).is_err());

        // Custom field limit
        assert!(validate_multipart_limits(3, 1024, Some(2)).is_err());
        assert!(validate_multipart_limits(2, 1024, Some(2)).is_ok());
    }

    #[test]
    fn test_validate_cargo_upload_structure() {
        // Valid structure
        let metadata_size = 1024;
        let crate_size = 10 * 1024 * 1024; // 10MB
        let payload_size = metadata_size + crate_size + 8;
        assert!(validate_cargo_upload_structure(payload_size, metadata_size, crate_size).is_ok());

        // Payload too large
        assert!(validate_cargo_upload_structure(MAX_REQUEST_BODY_SIZE + 1, 1024, 1024).is_err());

        // Metadata too large
        assert!(
            validate_cargo_upload_structure(1024 + 8 + 1024, MAX_METADATA_SIZE + 1, 1024).is_err()
        );

        // Crate file too large
        let large_crate_size = MAX_PACKAGE_FILE_SIZE as usize + 1;
        assert!(validate_cargo_upload_structure(
            large_crate_size + 1024 + 8,
            1024,
            large_crate_size
        )
        .is_err());

        // Payload smaller than expected
        assert!(validate_cargo_upload_structure(100, 1024, 1024).is_err());
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
    fn test_escape_shell_arg_for_docker() {
        // Docker arguments are escaped using the same shell logic
        assert_eq!(escape_shell_arg("simple"), "simple");
        assert_eq!(escape_shell_arg("path with spaces"), "'path with spaces'");
        assert_eq!(
            escape_shell_arg("dangerous$variable"),
            "'dangerous$variable'"
        );
        assert_eq!(
            escape_shell_arg("command`injection`"),
            "'command`injection`'"
        );
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
