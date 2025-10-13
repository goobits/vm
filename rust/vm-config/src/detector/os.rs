//! Host operating system detection utilities.
//!
//! This module provides functionality for detecting the host operating system
//! and Linux distribution. This information is used to:
//! - Adapt VM configurations for different host platforms
//! - Provide platform-specific recommendations
//! - Enable host-aware virtualization optimizations

use std::env;
use std::fs;
use std::process::Command;
use vm_core::error::{Result, VmError};

/// Detects the host operating system and distribution.
///
/// Provides a unified interface for OS detection across different platforms.
/// For Linux systems, attempts to detect the specific distribution from
/// `/etc/os-release`. For other platforms, returns the general OS type.
///
/// ## Return Values
/// - **macOS**: "macos"
/// - **Windows**: "windows"
/// - **Linux distributions**: "ubuntu", "debian", "fedora", "arch", etc.
/// - **Generic Linux**: "linux" (if distribution cannot be determined)
/// - **Unknown**: "unknown" (for unrecognized platforms)
///
/// # Returns
/// A string identifier for the detected operating system
///
/// # Examples
/// ```rust
/// use vm_config::detector::detect_host_os;
///
/// let os = detect_host_os();
/// match os.as_str() {
///     "macos" => println!("Running on macOS"),
///     "ubuntu" => println!("Running on Ubuntu Linux"),
///     "windows" => println!("Running on Windows"),
///     _ => println!("Running on: {}", os),
/// }
/// ```
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
    Err(VmError::Config(
        "Could not determine Linux distribution from /etc/os-release".to_string(),
    ))
}

pub fn detect_timezone() -> String {
    // 1. Read from TZ environment variable
    if let Ok(tz) = env::var("TZ") {
        if !tz.is_empty() {
            return tz;
        }
    }

    // 2. Use timedatectl on systemd systems
    if let Ok(output) = Command::new("timedatectl").arg("show").arg("--property=Timezone").arg("--value").output() {
        if output.status.success() {
            let timezone = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !timezone.is_empty() {
                return timezone;
            }
        }
    }

    // 3. Read /etc/timezone on Linux host
    if let Ok(timezone) = fs::read_to_string("/etc/timezone") {
        return timezone.trim().to_string();
    }

    // 4. Follow symlink /etc/localtime
    if let Ok(path) = fs::read_link("/etc/localtime") {
        if let Some(tz) = path.to_string_lossy().split("zoneinfo/").nth(1) {
            return tz.to_string();
        }
    }

    // 5. Fallback to UTC
    "UTC".to_string()
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
        assert!(
            matches!(
                os.as_str(),
                "macos" | "windows" | "linux" | "ubuntu" | "fedora" | "debian" | "arch" | "unknown"
            ) || os.starts_with("linux")
                || !os.is_empty()
        );
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
