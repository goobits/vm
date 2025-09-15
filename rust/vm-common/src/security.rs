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

    let mut safe_path_prefixes: Vec<String> = vec![
        "/home/".to_string(),
        "/tmp/".to_string(),
        "/var/tmp/".to_string(),
        "/workspace/".to_string(),
        "/opt/".to_string(),
        "/srv/".to_string(),
        "/usr/local/".to_string(),
        "/data/".to_string(),
        "/projects/".to_string(),
    ];

    if let Ok(current_dir) = std::env::current_dir() {
        if let Some(current_dir_str) = current_dir.to_str() {
            safe_path_prefixes.push(current_dir_str.to_string());
        }
    }

    if let Some(additional_paths) = additional_safe_paths {
        safe_path_prefixes.extend(additional_paths.iter().map(|s| s.to_string()));
    }

    let is_safe = safe_path_prefixes
        .iter()
        .any(|safe_prefix| resolved_path.starts_with(safe_prefix));

    if is_safe {
        Ok(())
    } else {
        Err(anyhow!(
            "Path '{:?}' is not in allowed safe locations",
            resolved_path
        ))
    }
}
