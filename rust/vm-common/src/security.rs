//! Security utilities for path validation and input sanitization.
//!
//! This module provides functions to safely handle user input, validate paths,
//! and prevent security vulnerabilities in file system operations. It includes
//! protection against path traversal attacks, shell injection, and unsafe file
//! system access patterns.

use crate::platform::portable_readlink;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Checks for dangerous shell metacharacters in a string.
pub fn check_dangerous_characters(input_string: &str, context: &str) -> Result<()> {
    // List of dangerous characters
    let dangerous_chars = [
        ';', '`', '$', '"', '|', '&', '>', '<', '(', ')', '{', '}', '*', '?', '[', ']', '~', '@',
        '#', '%',
    ];
    if input_string.chars().any(|c| dangerous_chars.contains(&c)) {
        return Err(anyhow!(
            "{} contains potentially dangerous characters",
            context
        ));
    }

    // Check for dangerous control characters
    if input_string.chars().any(|c| c.is_control()) {
        return Err(anyhow!("{} contains dangerous control characters", context));
    }

    Ok(())
}

/// Validates and resolves a path to its canonical form.
pub fn resolve_path_secure(input_path: &Path, must_exist: bool) -> Result<PathBuf> {
    if let Some(input_str) = input_path.to_str() {
        check_dangerous_characters(input_str, "path")?;
    } else {
        return Err(anyhow!("Path contains invalid UTF-8 characters"));
    }

    let resolved_path = if must_exist {
        portable_readlink(input_path)?
    } else {
        let parent_dir = input_path
            .parent()
            .ok_or_else(|| anyhow!("Cannot get parent directory of {:?}", input_path))?;
        let filename = input_path
            .file_name()
            .ok_or_else(|| anyhow!("Cannot get filename of {:?}", input_path))?;
        let resolved_parent = portable_readlink(parent_dir)?;
        resolved_parent.join(filename)
    };

    if resolved_path.to_str().unwrap_or("").contains("..") {
        return Err(anyhow!(
            "Resolved path contains parent directory references"
        ));
    }

    Ok(resolved_path)
}

/// Checks if a path is within a set of allowed safe directories.
pub fn is_path_in_safe_locations(
    check_path: &Path,
    additional_safe_paths: Option<&[&str]>,
) -> Result<()> {
    let resolved_path = resolve_path_secure(check_path, true)?;

    // Static safe path prefixes - no allocations needed
    const BASE_SAFE_PREFIXES: &[&str] = &[
        "/home/",
        "/workspace/",
        "/opt/",
        "/srv/",
        "/usr/local/",
        "/data/",
        "/projects/",
    ];

    // Platform-specific safe prefixes
    #[cfg(target_os = "macos")]
    const PLATFORM_SAFE_PREFIXES: &[&str] =
        &["/tmp/", "/var/tmp/", "/var/folders/", "/private/tmp/"];
    #[cfg(target_os = "linux")]
    const PLATFORM_SAFE_PREFIXES: &[&str] = &["/tmp/", "/var/tmp/", "/dev/shm/"];
    #[cfg(target_os = "windows")]
    const PLATFORM_SAFE_PREFIXES: &[&str] = &[];

    // Check against static prefixes first (no allocations)
    let is_safe = BASE_SAFE_PREFIXES
        .iter()
        .chain(PLATFORM_SAFE_PREFIXES.iter())
        .any(|&prefix| resolved_path.starts_with(prefix));

    if is_safe {
        return Ok(());
    }

    // Only allocate for dynamic paths if static check failed
    let temp_dir_path = std::env::temp_dir();
    let temp_dir = temp_dir_path.to_string_lossy();
    if resolved_path.starts_with(temp_dir.as_ref()) {
        return Ok(());
    }

    if let Ok(current_dir) = std::env::current_dir() {
        if let Some(current_dir_str) = current_dir.to_str() {
            if resolved_path.starts_with(current_dir_str) {
                return Ok(());
            }
        }
    }

    if let Some(additional_paths) = additional_safe_paths {
        if additional_paths
            .iter()
            .any(|&path| resolved_path.starts_with(path))
        {
            return Ok(());
        }
    }

    Err(anyhow!(
        "Path '{:?}' is not in allowed safe locations",
        resolved_path
    ))
}
