// Common utilities and types for VM Tool
// This crate provides shared functionality across VM Tool components

pub mod file_system;
pub mod log_context;
pub mod output_macros;
pub mod platform;
pub mod security;
pub mod structured_log;
pub mod temp;

// Standard library
use std::path::Path;
use std::process::Command;

// External crates
use anyhow::{Context, Result};

/// Common prelude for VM Tool modules
pub mod prelude {
    pub use anyhow::{Context, Result};
    pub use serde::{Deserialize, Serialize};
}

/// File operations utilities
pub struct FileOps;

impl FileOps {
    /// Read file contents with proper error context
    pub fn read_file_with_context(path: &Path) -> Result<String> {
        std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))
    }

    /// Read from file or stdin if path is "-"
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
    pub fn execute_command_with_args(cmd: &str, args: &[&str]) -> Result<String> {
        let output = Command::new(cmd)
            .args(args)
            .output()
            .with_context(|| format!("Failed to execute command: {}", cmd))?;

        if !output.status.success() {
            anyhow::bail!(
                "Command {} failed with exit code: {:?}",
                cmd,
                output.status.code()
            );
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }
}

/// Path utilities
pub struct PathOps;

impl PathOps {
    /// Get current directory with fallback, logging warnings
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
