use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;
use std::collections::HashSet;
use std::process::Command;

pub fn detect_cargo_packages(packages: &[String]) -> Result<Vec<(String, String)>> {
    let package_set: HashSet<&String> = packages.iter().collect();
    let mut results = Vec::new();

    // Get cargo install list
    let output = Command::new("cargo").args(["install", "--list"]).output()?;

    if !output.status.success() {
        return Ok(results);
    }

    let output_str = String::from_utf8(output.stdout)?;
    let installations = parse_cargo_install_list(&output_str)?;

    // Check each installation against requested packages in parallel
    let detections: Vec<_> = installations
        .par_iter()
        .filter_map(|(pkg_name, pkg_path)| {
            // Check if this package is in our requested list
            for requested_pkg in &package_set {
                if pkg_name == *requested_pkg {
                    // Verify the path exists and looks like a source directory
                    if std::path::Path::new(pkg_path).exists() && pkg_path.contains('/') {
                        return Some((pkg_name.clone(), pkg_path.clone()));
                    }
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
    let re = Regex::new(r"^([a-zA-Z0-9_-]+)\s+[^\(]*\(([^)]+)\):$")?;

    for line in output.lines() {
        if let Some(captures) = re.captures(line) {
            let pkg_name = captures
                .get(1)
                .expect("Regex group 1 should exist for cargo package name")
                .as_str()
                .to_string();
            let pkg_path = captures
                .get(2)
                .expect("Regex group 2 should exist for cargo package path")
                .as_str()
                .to_string();

            // Only include path-based installs (not registry installs)
            if pkg_path.contains('/') && !pkg_path.starts_with("registry+") {
                installations.push((pkg_name, pkg_path));
            }
        }
    }

    Ok(installations)
}