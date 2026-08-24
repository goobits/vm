use std::env;
use std::fs;
use std::process::Command;

pub(crate) fn operating_system() -> &'static str {
    env::consts::OS
}

pub(crate) fn architecture() -> &'static str {
    env::consts::ARCH
}

pub(crate) fn detect_host_os() -> String {
    match operating_system() {
        "linux" => detect_linux_distribution().unwrap_or_else(|| "linux".to_string()),
        "macos" => "macos".to_string(),
        "windows" => "windows".to_string(),
        _ => "unknown".to_string(),
    }
}

fn detect_linux_distribution() -> Option<String> {
    let release_info = fs::read_to_string("/etc/os-release").ok()?;
    release_info.lines().find_map(|line| {
        line.strip_prefix("ID=")
            .map(|id| id.trim_matches('"').to_lowercase())
    })
}

pub(crate) fn detect_timezone() -> String {
    if let Ok(timezone) = env::var("TZ") {
        if !timezone.is_empty() {
            return timezone;
        }
    }

    if let Ok(output) = Command::new("timedatectl")
        .args(["show", "--property=Timezone", "--value"])
        .output()
    {
        if output.status.success() {
            let timezone = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !timezone.is_empty() {
                return timezone;
            }
        }
    }

    if let Ok(timezone) = fs::read_to_string("/etc/timezone") {
        return timezone.trim().to_string();
    }

    if let Ok(path) = fs::read_link("/etc/localtime") {
        if let Some(timezone) = path.to_string_lossy().split("zoneinfo/").nth(1) {
            return timezone.to_string();
        }
    }

    "UTC".to_string()
}

pub(crate) fn current_uid() -> u32 {
    #[cfg(unix)]
    {
        non_root_id("SUDO_UID", nix::unistd::getuid().as_raw())
    }
    #[cfg(not(unix))]
    {
        1000
    }
}

pub(crate) fn current_gid() -> u32 {
    #[cfg(unix)]
    {
        non_root_id("SUDO_GID", nix::unistd::getgid().as_raw())
    }
    #[cfg(not(unix))]
    {
        1000
    }
}

#[cfg(unix)]
fn non_root_id(sudo_variable: &str, effective_id: u32) -> u32 {
    env::var(sudo_variable)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|id| *id > 0)
        .unwrap_or(if effective_id == 0 {
            1000
        } else {
            effective_id
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_facts_are_available() {
        assert!(!operating_system().is_empty());
        assert!(!architecture().is_empty());
        assert!(!detect_host_os().is_empty());
        assert!(!detect_timezone().is_empty());
        assert!(current_uid() > 0);
        assert!(current_gid() > 0);
    }
}
