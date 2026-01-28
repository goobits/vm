use rayon::prelude::*;
use std::collections::HashSet;
use std::process::Command;
use vm_core::error::{Result, VmError};

pub fn detect_cargo_packages(packages: &[String]) -> Result<Vec<(String, String)>> {
    let package_set: HashSet<&String> = packages.iter().collect();
    let mut results = Vec::new();

    // Get cargo install list
    let output = Command::new("cargo").args(["install", "--list"]).output()?;

    if !output.status.success() {
        return Ok(results);
    }

    let output_str = String::from_utf8(output.stdout)
        .map_err(|e| VmError::Internal(format!("Failed to parse cargo command output: {e}")))?;
    let installations = parse_cargo_install_list(&output_str)?;

    // Check each installation against requested packages in parallel
    let detections: Vec<_> = installations
        .par_iter()
        .filter_map(|(pkg_name, pkg_path)| {
            // Check if this package is in our requested list
            if package_set.contains(pkg_name) {
                // Verify the path exists and looks like a source directory
                if std::path::Path::new(pkg_path).exists() && pkg_path.contains('/') {
                    return Some((pkg_name.clone(), pkg_path.clone()));
                }
            }
            None
        })
        .collect();

    results.extend(detections);
    Ok(results)
}

fn parse_cargo_install_list(output: &str) -> Result<Vec<(String, String)>> {
    let mut installations = Vec::new();

    for line in output.lines() {
        // Parse: package_name v1.0.0 (/path/to/source):
        // Regex equivalent: ^([a-zA-Z0-9_-]+)\s+[^\(]*\(([^)]+)\):$

        // 1. Extract package name (chars until first whitespace)
        let Some(first_space_idx) = line.find(char::is_whitespace) else {
            continue;
        };

        let pkg_name_str = &line[..first_space_idx];

        // Validation: [a-zA-Z0-9_-]+
        if pkg_name_str.is_empty()
            || !pkg_name_str
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            continue;
        }

        // 2. Check line ending
        if !line.ends_with("):") {
            continue;
        }

        // 3. Find path start
        // Skip package name, search for first '('
        let Some(paren_relative_idx) = line[first_space_idx..].find('(') else {
            continue;
        };
        let paren_start_idx = first_space_idx + paren_relative_idx;

        // Path is between '(' and final '):'
        let path_start = paren_start_idx + 1;
        let path_end = line.len() - 2;

        if path_start >= path_end {
            continue;
        }

        let pkg_path = &line[path_start..path_end];

        // Regex ([^)]+) implies path cannot contain ')'
        if pkg_path.contains(')') {
            continue;
        }

        // Only include path-based installs (not registry installs)
        if pkg_path.contains('/') && !pkg_path.starts_with("registry+") {
            installations.push((pkg_name_str.to_string(), pkg_path.to_string()));
        }
    }

    Ok(installations)
}
