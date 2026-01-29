use rayon::prelude::*;
use serde_json::Value;
use std::collections::HashSet;
use std::process::Command;
use vm_core::error::{Result, VmError};

pub fn detect_pip_packages(packages: &[String]) -> Vec<(String, String)> {
    // Try different pip commands in parallel
    let pip_commands = vec!["pip", "pip3", "python3", "python"];

    let detections: Vec<_> = pip_commands
        .par_iter()
        .filter_map(|cmd| get_editable_packages_for_command(cmd).ok())
        .flatten()
        .collect();

    // Process detections and match against requested packages
    find_matching_packages(detections, packages)
}

fn find_matching_packages(
    detections: Vec<(String, String)>,
    requested_packages: &[String],
) -> Vec<(String, String)> {
    // Pre-process requested packages: normalize them (lower case, dashes to underscores)
    // We store the normalized name in a Set for O(1) lookup.
    let normalized_requests: HashSet<String> = requested_packages
        .iter()
        .map(|pkg| pkg.to_lowercase().replace('-', "_"))
        .collect();

    let mut results = Vec::new();
    let mut found_packages = HashSet::new();

    for (package_name, location) in detections {
        if found_packages.contains(&package_name) {
            continue; // Skip if already found
        }

        // Normalize detection name
        let normalized_name = package_name.to_lowercase().replace('-', "_");

        if normalized_requests.contains(&normalized_name) {
            results.push((package_name.clone(), location.clone()));
            found_packages.insert(package_name.clone());
        }
    }

    results
}

fn get_editable_packages_for_command(cmd: &str) -> Result<Vec<(String, String)>> {
    let output = match cmd {
        "pip" | "pip3" => Command::new(cmd)
            .args(["list", "-e", "--format=json"])
            .output()?,
        "python" | "python3" => Command::new(cmd)
            .args(["-m", "pip", "list", "-e", "--format=json"])
            .output()?,
        _ => return Ok(Vec::new()),
    };

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let output_str = String::from_utf8(output.stdout)
        .map_err(|e| VmError::Internal(format!("Failed to parse pip command output: {e}")))?;
    let json_data: Value = serde_json::from_str(&output_str)?;

    let mut packages = Vec::new();

    if let Value::Array(items) = json_data {
        for item in items {
            if let (Some(name), Some(location)) = (
                item.get("name").and_then(|n| n.as_str()),
                item.get("editable_project_location")
                    .and_then(|l| l.as_str()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matching_correctness() {
        // Requested packages: "requests", "My-Package", "Data_Science"
        let requested = vec![
            "requests".to_string(),
            "My-Package".to_string(),
            "Data_Science".to_string(),
        ];

        // Detected packages
        let detections = vec![
            ("requests".to_string(), "/lib/requests".to_string()), // Exact match
            ("my_package".to_string(), "/lib/my_package".to_string()), // Case + dash/underscore mismatch
            ("data-science".to_string(), "/lib/data-science".to_string()), // Case + underscore/dash mismatch
            ("other".to_string(), "/lib/other".to_string()),               // No match
        ];

        let results = find_matching_packages(detections, &requested);

        assert_eq!(results.len(), 3);
        let found_names: HashSet<String> = results.into_iter().map(|(n, _)| n).collect();
        assert!(found_names.contains("requests"));
        assert!(found_names.contains("my_package"));
        assert!(found_names.contains("data-science"));
    }
}
