//! Host and provider health checks with optional repairs.

use std::process::{Command, Stdio};

use anyhow::Result;
use vm_core::{vm_hint, vm_println, vm_progress, vm_success, vm_warning};

mod configuration;
mod provider;
mod resources;
mod ssh;

pub fn run_with_fix(
    fix: bool,
    provider_name: &str,
    configuration_error: Option<&str>,
) -> Result<()> {
    vm_progress!("Running diagnostics...");
    let mut all_ok = true;
    let mut issues_fixed = 0;

    if let Some(error) = configuration_error {
        vm_println!("  Configuration: invalid ({error})");
        all_ok = false;
    } else {
        vm_println!("  Configuration: ok");
    }

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

    match provider::validate(provider_name) {
        Ok(()) => vm_println!("  {}: ok", provider::label(provider_name)),
        Err(error) => {
            let error_text = error.to_string();
            let mut resolved = false;
            if error_text.contains("not installed") {
                vm_println!("  {}: not installed", provider::label(provider_name));
                match provider_name {
                    "tart" => vm_hint!("Install Tart from https://tart.run/"),
                    "podman" => vm_hint!("Install Podman from https://podman.io/docs/installation"),
                    _ => vm_hint!("Install Docker from https://docs.docker.com/get-docker/"),
                }
            } else if error_text.contains("not running") {
                vm_println!("  {}: not running", provider::label(provider_name));
                if fix && provider_name == "docker" {
                    if provider::start_docker() {
                        vm_println!("  Docker: started");
                        issues_fixed += 1;
                        resolved = true;
                    } else {
                        vm_hint!("Start Docker Desktop or run `sudo systemctl start docker`");
                    }
                } else if provider_name == "podman" {
                    vm_hint!("Start Podman with `podman machine start`");
                } else {
                    vm_hint!("Start Docker Desktop, or run `vm doctor --fix`");
                }
            } else if error_text.contains("permission") && provider_name == "docker" {
                vm_println!("  Docker: permission denied");
                if fix {
                    if provider::fix_docker_permissions() {
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
            } else if error_text.contains("known incompatible Tart release") {
                vm_println!("  Tart: incompatible version");
                vm_hint!("Use Tart 2.32.1 on this macOS host; Tart 2.35.0 is known to crash");
            } else {
                return Err(error.into());
            }
            if !resolved {
                all_ok = false;
            }
        }
    }

    match ssh::check_permissions() {
        Ok(()) => vm_println!("  SSH permissions: ok"),
        Err(message) => {
            vm_println!("  SSH permissions: {message}");
            if fix && ssh::fix_permissions() {
                vm_println!("  SSH permissions: fixed");
                issues_fixed += 1;
            } else if fix {
                vm_warning!("SSH permissions were not changed");
            }
        }
    }

    let port_conflicts = resources::port_conflicts();
    if port_conflicts.is_empty() {
        vm_println!("  Common ports: available");
    } else {
        vm_println!("  Common ports in use: {:?}", port_conflicts);
    }
    if let Some((used, limit)) = resources::file_descriptor_usage() {
        let percent = used.saturating_mul(100).checked_div(limit).unwrap_or(0);
        vm_println!("  Host file descriptors: {used}/{limit} ({percent}%)");
        if percent >= 85 {
            vm_warning!("Host file descriptors are near exhaustion");
            vm_hint!("Stop stale VMs, containers, browsers, or helper processes before retrying");
            all_ok = false;
        }
    }

    vm_println!("  vm binary: ok");
    match configuration::check_directory() {
        Ok(()) => vm_println!("  Config directory: ok"),
        Err(message) => {
            vm_println!("  Config directory: {message}");
            if fix && configuration::create_directory() {
                vm_println!("  Config directory: created");
                issues_fixed += 1;
            }
        }
    }

    match crate::commands::packages::diagnose_client_access(fix) {
        Ok(Some(true)) => vm_println!("  Package infrastructure access: ok"),
        Ok(Some(false)) => {
            vm_println!("  Package infrastructure access: stale");
            vm_hint!("Run `vm doctor --fix` to repair managed package credentials");
            all_ok = false;
        }
        Ok(None) => {}
        Err(error) => {
            vm_println!("  Package infrastructure access: {error}");
            all_ok = false;
        }
    }

    if all_ok {
        vm_success!("All checks passed; vm is ready");
        return Ok(());
    }
    if issues_fixed > 0 {
        vm_println!("  Fixed {issues_fixed} issue(s)");
        vm_warning!("Some issues remain");
    }
    if !fix {
        vm_hint!("Run `vm doctor --fix` to attempt repairs");
    }
    anyhow::bail!("Diagnostics found unresolved issues")
}
