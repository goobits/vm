//! Package client operations
//!
//! This module provides CLI functionality for managing packages across different
//! package managers (Python/pip, Node.js/npm, Rust/Cargo).

mod builders;
mod commands;
mod detection;

// Re-export all public functions and types to maintain API compatibility
pub use builders::*;
pub use commands::*;
pub use detection::*;
