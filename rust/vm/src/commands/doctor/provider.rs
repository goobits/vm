use std::process::Command;

use vm_config::config::ProviderName;
use vm_provider::{validate_provider_environment, VmResult};

pub(super) fn label(provider: &str) -> &str {
    match provider {
        "docker" => "Docker",
        "podman" => "Podman",
        "tart" => "Tart",
        other => other,
    }
}

pub(super) fn validate(provider: &str) -> VmResult<()> {
    validate_provider_environment(&ProviderName::from(provider))
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
