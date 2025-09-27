//! Cargo registry implementation
//!
//! This module provides Cargo package registry functionality including
//! index management, package uploads, downloads, and metadata operations.

use serde_json::Value;

mod handlers;
mod index;
mod parsing;
mod storage;

#[cfg(test)]
mod tests;

/// Metadata extracted from a crate upload
#[derive(Debug)]
pub struct CrateMetadata {
    pub name: String,
    pub version: String,
    pub deps: Value,
    pub features: Value,
}

// Re-export all public functions and types to maintain API compatibility
pub use handlers::*;
pub use index::*;
pub use parsing::*;
pub use storage::*;
