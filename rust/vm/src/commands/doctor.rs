//! VM Doctor command - Health checks and auto-fixes
//!
//! This module provides diagnostic checks for the VM tool and
//! optionally attempts to fix common issues.

use anyhow::Result;
use std::process::{Command, Stdio};
use vm_core::{vm_hint, vm_println, vm_progress, vm_success, vm_warning};
use vm_provider::docker::validate_docker_environment;

/// Run diagnostics with optional auto-fix
pub fn run_with_fix(fix: bool, provider: &str) -> Result<()> {
    run_diagnostics(fix, provider)
}

/// Internal diagnostic runner
fn run_diagnostics(fix: bool, provider: &str) -> Result<()> {
    vm_progress!("Running diagnostics...");
    let mut all_ok = true;
    let mut issues_fixed = 0;

    if Command::new("rustc")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        vm_println!("  Rust: ok");
    } else {
        vm_println!("  Rust: unavailable (optional)");
    }

    match validate_provider_environment(provider) {
        Ok(_) => vm_println!("  {}: ok", provider_label(provider)),
        Err(e) => {
            let error_str = e.to_string();
            let mut resolved = false;
            if error_str.contains("not installed") {
                vm_println!("  {}: not installed", provider_label(provider));
                match provider {
                    "tart" => vm_hint!("Install Tart from https://tart.run/"),
                    "podman" => vm_hint!("Install Podman from https://podman.io/docs/installation"),
                    _ => vm_hint!("Install Docker from https://docs.docker.com/get-docker/"),
                }
            } else if error_str.contains("not running") {
                vm_println!("  {}: not running", provider_label(provider));
                if fix && provider == "docker" {
                    if try_start_docker() {
                        vm_println!("  Docker: started");
                        issues_fixed += 1;
                        resolved = true;
                    } else {
                        vm_hint!("Start Docker Desktop or run `sudo systemctl start docker`");
                    }
                } else if provider == "podman" {
                    vm_hint!("Start Podman with `podman machine start`");
                } else {
                    vm_hint!("Start Docker Desktop, or run `vm doctor --fix`");
                }
            } else if error_str.contains("permission") && provider == "docker" {
                vm_println!("  Docker: permission denied");
                if fix {
                    if try_fix_docker_permissions() {
                        vm_println!("  Docker: added user to docker group");
                        vm_warning!("Log out and back in before retrying Docker");
                        issues_fixed += 1;
                        resolved = true;
                    } else {
                        vm_hint!("Run `sudo usermod -aG docker $USER && newgrp docker`");
                    }
                } else {
                    vm_hint!("Run `vm doctor --fix`");
                }
            } else if error_str.contains("known incompatible Tart release") {
                vm_println!("  Tart: incompatible version");
                vm_hint!("Use Tart 2.32.1 on this macOS host; Tart 2.35.0 is known to crash");
            } else {
                return Err(e.into());
            }
            if !resolved {
                all_ok = false;
            }
        }
    }

    match check_ssh_permissions() {
        Ok(_) => vm_println!("  SSH permissions: ok"),
        Err(msg) => {
            vm_println!("  SSH permissions: {msg}");
            if fix && try_fix_ssh_permissions() {
                vm_println!("  SSH permissions: fixed");
                issues_fixed += 1;
            } else if fix {
                vm_warning!("SSH permissions were not changed");
            }
        }
    }

    let port_conflicts = check_port_conflicts();
    if port_conflicts.is_empty() {
        vm_println!("  Common ports: available");
    } else {
        vm_println!("  Common ports in use: {:?}", port_conflicts);
    }

    if let Some((used, limit)) = host_file_descriptor_usage() {
        let percent = used.saturating_mul(100).checked_div(limit).unwrap_or(0);
        vm_println!("  Host file descriptors: {used}/{limit} ({percent}%)");
        if percent >= 85 {
            vm_warning!("Host file descriptors are near exhaustion");
            vm_hint!("Stop stale VMs, containers, browsers, or helper processes before retrying");
            all_ok = false;
        }
    }

    vm_println!("  vm binary: ok");

    match check_config_directory() {
        Ok(_) => vm_println!("  Config directory: ok"),
        Err(msg) => {
            vm_println!("  Config directory: {msg}");
            if fix && try_create_config_directory() {
                vm_println!("  Config directory: created");
                issues_fixed += 1;
            }
        }
    }

    if all_ok {
        vm_success!("All checks passed; vm is ready");
    } else {
        if issues_fixed > 0 {
            vm_println!("  Fixed {issues_fixed} issue(s)");
            vm_warning!("Some issues remain");
        }
        if !fix {
            vm_hint!("Run `vm doctor --fix` to attempt repairs");
        }
        anyhow::bail!("Diagnostics found unresolved issues");
    }

    Ok(())
}

fn provider_label(provider: &str) -> &str {
    match provider {
        "docker" => "Docker",
        "podman" => "Podman",
        "tart" => "Tart",
        other => other,
    }
}

fn validate_provider_environment(provider: &str) -> vm_provider::VmResult<()> {
    match provider {
        "docker" | "podman" => validate_docker_environment(provider),
        "tart" => {
            let output = Command::new("tart")
                .arg("--version")
                .output()
                .map_err(|error| {
                    vm_provider::VmError::Dependency(format!("tart is not installed: {error}"))
                })?;
            if !output.status.success() {
                return Err(vm_provider::VmError::Provider(
                    "tart is not available".to_string(),
                ));
            }
            let version = String::from_utf8_lossy(&output.stdout);
            if tart_version_number(&version) == Some("2.35.0") {
                Err(vm_provider::VmError::Provider(
                    "known incompatible Tart release 2.35.0".to_string(),
                ))
            } else {
                Ok(())
            }
        }
        other => Err(vm_provider::VmError::Provider(format!(
            "Unknown provider '{other}'"
        ))),
    }
}

fn tart_version_number(output: &str) -> Option<&str> {
    output.split_whitespace().find(|value| {
        value
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_digit())
    })
}

#[cfg(target_os = "macos")]
fn host_file_descriptor_usage() -> Option<(u64, u64)> {
    let output = Command::new("/usr/sbin/sysctl")
        .args(["-n", "kern.num_files", "kern.maxfiles"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_descriptor_values(&String::from_utf8_lossy(&output.stdout)))?
}

#[cfg(target_os = "linux")]
fn host_file_descriptor_usage() -> Option<(u64, u64)> {
    let values = std::fs::read_to_string("/proc/sys/fs/file-nr").ok()?;
    parse_linux_descriptor_values(&values)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn host_file_descriptor_usage() -> Option<(u64, u64)> {
    None
}

#[cfg(any(target_os = "macos", test))]
fn parse_descriptor_values(values: &str) -> Option<(u64, u64)> {
    let mut values = values
        .split_whitespace()
        .filter_map(|value| value.parse().ok());
    Some((values.next()?, values.next()?))
}

#[cfg(target_os = "linux")]
fn parse_linux_descriptor_values(values: &str) -> Option<(u64, u64)> {
    let mut values = values
        .split_whitespace()
        .filter_map(|value| value.parse::<u64>().ok());
    let allocated = values.next()?;
    let unused = values.next()?;
    let limit = values.next()?;
    Some((allocated.saturating_sub(unused), limit))
}

/// Check SSH directory and key permissions
fn check_ssh_permissions() -> Result<(), String> {
    let home = match dirs::home_dir() {
        Some(h) => h,
        None => return Err("Could not determine home directory".to_string()),
    };

    let ssh_dir = home.join(".ssh");
    if !ssh_dir.exists() {
        return Err("SSH directory doesn't exist".to_string());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata =
            std::fs::metadata(&ssh_dir).map_err(|e| format!("Cannot read SSH directory: {}", e))?;
        let mode = metadata.permissions().mode() & 0o777;

        if mode != 0o700 {
            return Err(format!(
                "SSH directory has wrong permissions: {:o} (should be 700)",
                mode
            ));
        }

        // Check id_rsa if it exists
        let id_rsa = ssh_dir.join("id_rsa");
        if id_rsa.exists() {
            let key_metadata =
                std::fs::metadata(&id_rsa).map_err(|e| format!("Cannot read SSH key: {}", e))?;
            let key_mode = key_metadata.permissions().mode() & 0o777;

            if key_mode != 0o600 {
                return Err(format!(
                    "SSH key has wrong permissions: {:o} (should be 600)",
                    key_mode
                ));
            }
        }

        // Check id_ed25519 if it exists
        let id_ed25519 = ssh_dir.join("id_ed25519");
        if id_ed25519.exists() {
            let key_metadata = std::fs::metadata(&id_ed25519)
                .map_err(|e| format!("Cannot read SSH key: {}", e))?;
            let key_mode = key_metadata.permissions().mode() & 0o777;

            if key_mode != 0o600 {
                return Err(format!(
                    "SSH key (ed25519) has wrong permissions: {:o} (should be 600)",
                    key_mode
                ));
            }
        }
    }

    Ok(())
}

/// Try to fix SSH permissions
fn try_fix_ssh_permissions() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return false,
        };

        let ssh_dir = home.join(".ssh");
        if !ssh_dir.exists() {
            return false;
        }

        // Fix directory permissions
        if std::fs::set_permissions(&ssh_dir, std::fs::Permissions::from_mode(0o700)).is_err() {
            return false;
        }

        // Fix key permissions
        for key_name in &["id_rsa", "id_ed25519", "id_ecdsa"] {
            let key_path = ssh_dir.join(key_name);
            if key_path.exists() {
                let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
            }
        }

        true
    }

    #[cfg(not(unix))]
    {
        false
    }
}

/// Check for common port conflicts
fn check_port_conflicts() -> Vec<u16> {
    let common_ports = vec![3000, 5432, 6379, 8080, 27017, 3306];
    let mut conflicts = Vec::new();

    for port in common_ports {
        if is_port_in_use(port) {
            conflicts.push(port);
        }
    }

    conflicts
}

/// Check if a port is in use
fn is_port_in_use(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_err()
}

/// Try to start Docker daemon
fn try_start_docker() -> bool {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("sudo")
            .args(["systemctl", "start", "docker"])
            .output();

        if let Ok(out) = output {
            if out.status.success() {
                // Wait a moment for Docker to start
                std::thread::sleep(std::time::Duration::from_secs(2));
                return validate_docker_environment("docker").is_ok();
            }
        }
        false
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, we can try to open Docker Desktop
        let output = Command::new("open").args(["-a", "Docker"]).output();

        if let Ok(out) = output {
            if out.status.success() {
                // Wait for Docker to start
                std::thread::sleep(std::time::Duration::from_secs(5));
                return validate_docker_environment("docker").is_ok();
            }
        }
        false
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

/// Try to fix Docker permissions
fn try_fix_docker_permissions() -> bool {
    #[cfg(target_os = "linux")]
    {
        let username = std::env::var("USER").unwrap_or_default();
        if username.is_empty() {
            return false;
        }

        let output = Command::new("sudo")
            .args(["usermod", "-aG", "docker", &username])
            .output();

        if let Ok(out) = output {
            return out.status.success();
        }
        false
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Check config directory exists
fn check_config_directory() -> Result<(), String> {
    let config_dir = match vm_core::user_paths::user_config_dir() {
        Ok(dir) => dir,
        Err(_) => return Err("Cannot determine config directory".to_string()),
    };

    if !config_dir.exists() {
        return Err(format!(
            "Config directory doesn't exist: {}",
            config_dir.display()
        ));
    }

    Ok(())
}

/// Try to create config directory
fn try_create_config_directory() -> bool {
    let config_dir = match vm_core::user_paths::user_config_dir() {
        Ok(dir) => dir,
        Err(_) => return false,
    };

    std::fs::create_dir_all(&config_dir).is_ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_descriptor_values, tart_version_number};

    #[test]
    fn parses_tart_version_output_without_assuming_a_prefix() {
        assert_eq!(tart_version_number("tart 2.32.1\n"), Some("2.32.1"));
        assert_eq!(tart_version_number("2.35.0\n"), Some("2.35.0"));
        assert_eq!(tart_version_number("tart unknown\n"), None);
    }

    #[test]
    fn parses_macos_file_descriptor_counters() {
        assert_eq!(
            parse_descriptor_values("8000\n10000\n"),
            Some((8000, 10000))
        );
        assert_eq!(parse_descriptor_values("unknown"), None);
    }
}
