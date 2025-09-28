//! Common utilities and types for VM Tool
//!
//! This crate provides shared functionality across VM Tool components, including
//! file operations, command execution utilities, output macros, and error handling.
//! It serves as the foundation for all other VM Tool crates.

pub mod command_stream;
pub mod error_messages;
pub mod error_traits;
pub mod errors;
pub mod file_system;
pub mod migrations;
pub mod output_macros;
pub mod platform;
pub mod security;
pub mod system_check;
pub mod temp_dir;
pub mod tracing_init;
pub mod user_paths;
pub mod yaml_utils;

// Re-export vm-messages for easy access
pub use vm_messages as messages;

// Standard library
use std::path::Path;
use std::process::Command;

// External crates
use anyhow::{Context, Result};

// Macros are automatically available at crate root via #[macro_export]

/// Common prelude for VM Tool modules
pub mod prelude {
    pub use anyhow::{Context, Result};
    pub use serde::{Deserialize, Serialize};
}

/// File operations utilities
pub struct FileOps;

impl FileOps {
    /// Read file contents with proper error context
    ///
    /// # Arguments
    /// * `path` - The path to the file to read
    ///
    /// # Returns
    /// The file contents as a String, or an error with context if the operation fails
    #[must_use = "file read results should be used"]
    pub fn read_file_with_context(path: &Path) -> Result<String> {
        std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))
    }

    /// Read from file or stdin if path is "-"
    ///
    /// # Arguments
    /// * `path` - The path to read from. If "-", reads from stdin
    ///
    /// # Returns
    /// The contents as a String, or an error if the operation fails
    #[must_use = "file or stdin read results should be used"]
    pub fn read_file_or_stdin(path: &Path) -> Result<String> {
        if path.to_str() == Some("-") {
            use std::io::Read;
            let mut buffer = String::new();
            std::io::stdin()
                .read_to_string(&mut buffer)
                .with_context(|| "Failed to read from stdin")?;
            Ok(buffer)
        } else {
            Self::read_file_with_context(path)
        }
    }
}

/// Command execution utilities
pub struct CommandOps;

impl CommandOps {
    /// Execute a command with arguments and return stdout
    ///
    /// # Arguments
    /// * `cmd` - The command to execute
    /// * `args` - The command line arguments
    ///
    /// # Returns
    /// The stdout output as a String, or an error if the command fails
    #[must_use = "command execution results should be used"]
    pub fn execute_command_with_args(cmd: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(cmd)
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute command: {}", cmd))?;

        if !output.status.success() {
            vm_error!(
                "Command {} failed with exit code: {:?}",
                cmd,
                output.status.code()
            );
            return Err(anyhow::anyhow!("Command failed"));
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }
}

/// Path utilities
pub struct PathOps;

impl PathOps {
    /// Get current directory with fallback, logging warnings
    ///
    /// # Returns
    /// The current directory path, or "." as fallback if current directory cannot be determined
    pub fn current_dir_with_fallback() -> std::path::PathBuf {
        std::env::current_dir().unwrap_or_else(|e| {
            eprintln!(
                "Warning: Could not get current directory: {}. Using '.' as fallback",
                e
            );
            std::path::PathBuf::from(".")
        })
    }
}
