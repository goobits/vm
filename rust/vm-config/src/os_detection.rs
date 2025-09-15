use anyhow::Result;
use std::fs;

/// Detects the host operating system and distribution.
///
/// Returns a string like "macos", "ubuntu", "fedora", "windows", etc.
pub fn detect_host_os() -> String {
    match std::env::consts::OS {
        "macos" => "macos".to_string(),
        "windows" => "windows".to_string(),
        "linux" => detect_linux_distro().unwrap_or_else(|_| "linux".to_string()),
        _ => "unknown".to_string(),
    }
}

/// Reads /etc/os-release to determine the Linux distribution.
fn detect_linux_distro() -> Result<String> {
    let release_info = fs::read_to_string("/etc/os-release")?;
    for line in release_info.lines() {
        if let Some(id) = line.strip_prefix("ID=") {
            return Ok(id.trim_matches('"').to_lowercase());
        }
    }
    Err(anyhow::anyhow!("Could not determine Linux distribution from /etc/os-release"))
}
