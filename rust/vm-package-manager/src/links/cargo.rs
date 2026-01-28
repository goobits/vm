use rayon::prelude::*;
use regex::Regex;
use std::collections::HashSet;
use std::process::Command;
use std::sync::OnceLock;
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

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

    // Regex to match cargo install list format:
    // package_name v1.0.0 (/path/to/source):
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^([a-zA-Z0-9_-]+)\s+[^\(]*\(([^)]+)\):$").expect("Failed to compile regex")
    });

    for line in output.lines() {
        if let Some(captures) = re.captures(line) {
            let pkg_name = match captures.get(1) {
                Some(m) => m.as_str().to_string(),
                None => {
                    vm_error!("Malformed cargo metadata line: missing package name");
                    continue;
                }
            };
            let pkg_path = match captures.get(2) {
                Some(m) => m.as_str(),
                None => {
                    vm_error!("Malformed cargo metadata line: missing package path");
                    continue;
                }
            };

            // Only include path-based installs (not registry installs)
            if pkg_path.contains('/') && !pkg_path.starts_with("registry+") {
                installations.push((pkg_name, pkg_path.to_string()));
            }
        }
    }

    Ok(installations)
}
