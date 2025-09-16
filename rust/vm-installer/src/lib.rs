//! VM installer library.
//!
//! This library provides installation functionality for the VM tool,
//! including platform detection, binary building, and PATH management.

pub mod dependencies;
pub mod installer;
pub mod platform;
pub mod prompt;

// Re-export key functions for testing and external use
pub use installer::install;
pub use platform::{detect_platform_string, ensure_path};
