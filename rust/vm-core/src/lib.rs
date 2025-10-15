//! # vm-core
//!
//! The `vm-core` crate provides the foundational building blocks for the Goobits VM command-line tool.
//! It offers essential utilities for file system operations, command execution, error handling,
//! system resource checking, and platform-specific path resolution. This crate is designed to be
//! a dependency for other components in the VM workspace, offering a consistent and robust
_//! toolkit for common tasks.

pub mod command_stream;
pub mod error;
pub mod file_system;
pub mod output_macros;
pub mod project;
pub mod system_check;
pub mod temp_dir;
pub mod user_paths;

// Re-export system resource detection functions for convenience
pub use system_check::{check_system_resources, get_cpu_core_count, get_total_memory_gb};
