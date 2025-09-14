// Common utilities and types for VM Tool
// This crate provides shared functionality across VM Tool components

/// Common error type for VM operations
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;