//! # Validation Error Types

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
    PathTraversal { path: String },

    #[error("Absolute path not allowed: {path}")]
    AbsolutePath { path: String },

    #[error("Path depth exceeds maximum: {actual} > {max}")]
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
