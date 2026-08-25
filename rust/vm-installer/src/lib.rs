//! VM installer library.
//!
//! This library provides installation functionality for the VM tool,
//! including platform detection, binary building, and PATH management.

mod build;
mod completion;
mod dependencies;
mod installer;
mod platform;
mod plugins;
mod prompt;

pub use dependencies::check as check_dependencies;
pub use installer::install;
pub use platform::{detect_platform_string, ensure_path};
