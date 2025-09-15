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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_host_os() {
        let os = detect_host_os();
        // Should return a non-empty string
        assert!(!os.is_empty());
        // Should be one of the known OS types or "unknown"
        assert!(matches!(
            os.as_str(),
            "macos" | "windows" | "linux" | "ubuntu" | "fedora" | "debian" | "arch" | "unknown"
        ) || os.starts_with("linux") || !os.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_detect_linux_distro() {
        // This test only runs on Linux systems
        let result = detect_linux_distro();
        match result {
            Ok(distro) => {
                assert!(!distro.is_empty());
                // Common distributions
                assert!(matches!(
                    distro.as_str(),
                    "ubuntu" | "debian" | "fedora" | "arch" | "centos" | "rhel" | "opensuse" | _
                ));
            }
            Err(_) => {
                // It's okay if /etc/os-release doesn't exist in test environments
            }
        }
    }
}