use anyhow::Result;
use rayon::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
use std::process::Command;

pub fn detect_pip_packages(packages: &[String]) -> Result<Vec<(String, String)>> {
    let package_set: HashSet<&String> = packages.iter().collect();
    let mut results = Vec::new();

    // Try different pip commands in parallel
    let pip_commands = vec!["pip", "pip3", "python3", "python"];

    let detections: Vec<_> = pip_commands.par_iter()
        .filter_map(|cmd| {
            if let Ok(pip_packages) = get_editable_packages_for_command(cmd) {
                Some(pip_packages)
            } else {
                None
            }
        })
        .flatten()
        .collect();

    // Process detections and match against requested packages
    let mut found_packages = HashSet::new();
    for (package_name, location) in detections {
        if found_packages.contains(&package_name) {
            continue; // Skip if already found
        }

        // Check if this package matches any requested packages (case-insensitive, handle dashes/underscores)
        for requested_pkg in &package_set {
            if package_matches(&package_name, requested_pkg) {
                results.push((package_name.clone(), location.clone()));
                found_packages.insert(package_name.clone());
                break;
            }
        }
    }

    Ok(results)
}

fn get_editable_packages_for_command(cmd: &str) -> Result<Vec<(String, String)>> {
    let output = match cmd {
        "pip" | "pip3" => {
            Command::new(cmd)
                .args(&["list", "-e", "--format=json"])
                .output()?
        }
        "python" | "python3" => {
            Command::new(cmd)
                .args(&["-m", "pip", "list", "-e", "--format=json"])
                .output()?
        }
        _ => return Ok(Vec::new()),
    };

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let output_str = String::from_utf8(output.stdout)?;
    let json_data: Value = serde_json::from_str(&output_str)?;

    let mut packages = Vec::new();

    if let Value::Array(items) = json_data {
        for item in items {
            if let (Some(name), Some(location)) = (
                item.get("name").and_then(|n| n.as_str()),
                item.get("editable_project_location").and_then(|l| l.as_str())
            ) {
                // Verify the location exists
                if std::path::Path::new(location).exists() {
                    packages.push((name.to_string(), location.to_string()));
                }
            }
        }
    }

    Ok(packages)
}

fn package_matches(package_name: &str, requested_name: &str) -> bool {
    // Convert to lowercase for comparison
    let name_lower = package_name.to_lowercase();
    let req_lower = requested_name.to_lowercase();

    // Normalize dashes/underscores
    let name_normalized = name_lower.replace('-', "_");
    let req_normalized = req_lower.replace('-', "_");

    // Check various combinations
    name_lower == req_lower ||
    name_lower == req_normalized ||
    name_normalized == req_lower ||
    name_normalized == req_normalized
}