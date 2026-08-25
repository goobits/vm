use std::process::Command;

use vm_config::config::ProviderName;
use vm_provider::{validate_container_environment, ContainerEngine, VmError, VmResult};

pub(super) fn label(provider: &str) -> &str {
    match provider {
        "docker" => "Docker",
        "podman" => "Podman",
        "tart" => "Tart",
        other => other,
    }
}

pub(super) fn validate(provider: &str) -> VmResult<()> {
    match provider {
        "docker" | "podman" => {
            let engine = ContainerEngine::detect(&ProviderName::from(provider))?;
            validate_container_environment(engine)
        }
        "tart" => {
            let output = Command::new("tart")
                .arg("--version")
                .output()
                .map_err(|error| VmError::Dependency(format!("tart is not installed: {error}")))?;
            if !output.status.success() {
                return Err(VmError::Provider("tart is not available".to_string()));
            }
            let version = String::from_utf8_lossy(&output.stdout);
            if tart_version_number(&version) == Some("2.35.0") {
                Err(VmError::Provider(
                    "known incompatible Tart release 2.35.0".to_string(),
                ))
            } else {
                Ok(())
            }
        }
        other => Err(VmError::Provider(format!("Unknown provider '{other}'"))),
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

pub(super) fn start_docker() -> bool {
    #[cfg(target_os = "linux")]
    {
        let started = Command::new("sudo")
            .args(["systemctl", "start", "docker"])
            .output()
            .is_ok_and(|output| output.status.success());
        if started {
            std::thread::sleep(std::time::Duration::from_secs(2));
            return validate("docker").is_ok();
        }
        false
    }

    #[cfg(target_os = "macos")]
    {
        let started = Command::new("open")
            .args(["-a", "Docker"])
            .output()
            .is_ok_and(|output| output.status.success());
        if started {
            std::thread::sleep(std::time::Duration::from_secs(5));
            return validate("docker").is_ok();
        }
        false
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

pub(super) fn fix_docker_permissions() -> bool {
    #[cfg(target_os = "linux")]
    {
        let username = std::env::var("USER").unwrap_or_default();
        !username.is_empty()
            && Command::new("sudo")
                .args(["usermod", "-aG", "docker", &username])
                .output()
                .is_ok_and(|output| output.status.success())
    }

    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::tart_version_number;

    #[test]
    fn parses_tart_version_without_assuming_a_prefix() {
        assert_eq!(tart_version_number("tart 2.32.1\n"), Some("2.32.1"));
        assert_eq!(tart_version_number("2.35.0\n"), Some("2.35.0"));
        assert_eq!(tart_version_number("tart unknown\n"), None);
    }
}
