use anyhow::Result;
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn detect_npm_packages(packages: &[String]) -> Result<Vec<(String, String)>> {
    let mut results = Vec::new();
    let mut found_packages = HashSet::new();

    // Convert packages to set for fast lookup
    let package_set: HashSet<&String> = packages.iter().collect();

    // Check npm root -g (global npm directory)
    if let Ok(npm_root) = get_npm_root() {
        let npm_detections = check_npm_directory(&npm_root, &package_set)?;
        for (package, path) in npm_detections {
            if !found_packages.contains(&package) {
                results.push((package.clone(), path));
                found_packages.insert(package);
            }
        }
    }

    // Check NVM directories in parallel
    if let Some(nvm_dir) = get_nvm_versions_dir() {
        let nvm_detections = check_nvm_directories(&nvm_dir, &package_set, &found_packages)?;
        for (package, path) in nvm_detections {
            if !found_packages.contains(&package) {
                results.push((package.clone(), path));
                found_packages.insert(package);
            }
        }
    }

    Ok(results)
}

fn get_npm_root() -> Result<PathBuf> {
    let output = Command::new("npm")
        .args(&["root", "-g"])
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get npm root directory");
    }

    let root_str = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(PathBuf::from(root_str))
}

fn get_nvm_versions_dir() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    let nvm_versions = PathBuf::from(home).join(".nvm/versions/node");

    if nvm_versions.exists() {
        Some(nvm_versions)
    } else {
        None
    }
}

fn check_npm_directory(npm_root: &Path, package_set: &HashSet<&String>) -> Result<Vec<(String, String)>> {
    let mut results = Vec::new();

    if !npm_root.exists() {
        return Ok(results);
    }

    // Use parallel iteration over packages for performance
    let detections: Vec<_> = package_set.par_iter()
        .filter_map(|package| {
            let link_path = npm_root.join(package);
            if link_path.is_symlink() {
                if let Ok(target_path) = std::fs::read_link(&link_path) {
                    // Convert to absolute path if relative
                    let absolute_target = if target_path.is_absolute() {
                        target_path
                    } else {
                        npm_root.join(target_path)
                    };

                    if absolute_target.exists() {
                        return Some(((*package).clone(), absolute_target.to_string_lossy().to_string()));
                    }
                }
            }
            None
        })
        .collect();

    results.extend(detections);
    Ok(results)
}

fn check_nvm_directories(
    nvm_versions_dir: &Path,
    package_set: &HashSet<&String>,
    already_found: &HashSet<String>,
) -> Result<Vec<(String, String)>> {
    let mut results = Vec::new();

    if !nvm_versions_dir.exists() {
        return Ok(results);
    }

    // Get all Node.js version directories
    let version_dirs: Vec<_> = std::fs::read_dir(nvm_versions_dir)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                let node_modules = path.join("lib/node_modules");
                if node_modules.exists() {
                    Some(node_modules)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect();

    // Check each version directory in parallel
    let detections: Vec<_> = version_dirs.par_iter()
        .flat_map(|node_modules| {
            package_set.par_iter()
                .filter_map(|package| {
                    // Skip if we already found this package
                    if already_found.contains(*package) {
                        return None;
                    }

                    let link_path = node_modules.join(package);
                    if link_path.is_symlink() {
                        if let Ok(target_path) = std::fs::read_link(&link_path) {
                            // Convert to absolute path if relative
                            let absolute_target = if target_path.is_absolute() {
                                target_path
                            } else {
                                node_modules.join(target_path)
                            };

                            if absolute_target.exists() {
                                return Some(((*package).clone(), absolute_target.to_string_lossy().to_string()));
                            }
                        }
                    }
                    None
                })
        })
        .collect();

    results.extend(detections);
    Ok(results)
}