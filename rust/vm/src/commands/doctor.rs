//! Doctor command handler
//!
//! This module provides diagnostic functionality to check the health and
//! configuration of the VM environment, including system dependencies,
//! configuration validation, and background service status.

use crate::error::{VmError, VmResult};
use std::path::PathBuf;
use std::process::Command;
use vm_cli::msg;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{user_paths, vm_error, vm_println, vm_success};
use vm_messages::messages::MESSAGES;

/// Handle the doctor command - perform comprehensive health checks
pub async fn handle_doctor_command(global_config: GlobalConfig) -> VmResult<()> {
    vm_println!("{}", MESSAGES.vm_doctor_header);
    vm_println!();

    let mut all_checks_passed = true;

    // 1. Configuration validation
    vm_println!("{}", MESSAGES.vm_doctor_config_section);
    if !check_configuration().await {
        all_checks_passed = false;
    }
    if !check_legacy_config_paths() {
        all_checks_passed = false;
    }
    vm_println!();

    // 2. System dependencies
    vm_println!("{}", MESSAGES.vm_doctor_deps_section);
    if !check_system_dependencies().await {
        all_checks_passed = false;
    }
    vm_println!();

    // 3. Background services
    vm_println!("{}", MESSAGES.vm_doctor_services_section);
    if !check_background_services(&global_config).await {
        all_checks_passed = false;
    }
    vm_println!();

    // Final summary
    vm_println!("{}", MESSAGES.vm_doctor_summary_separator);
    if all_checks_passed {
        vm_success!("{}", MESSAGES.vm_doctor_all_passed);
    } else {
        vm_error!("{}", MESSAGES.vm_doctor_some_failed);
        return Err(VmError::validation(
            "Health check failed - see diagnostic output above",
            None::<String>,
        ));
    }

    Ok(())
}

/// Check and validate the VM configuration
async fn check_configuration() -> bool {
    let mut config_ok = true;

    // Try to load configuration
    match VmConfig::load(None) {
        Ok(config) => {
            vm_success!("{}", MESSAGES.vm_doctor_config_loaded);

            // Run validation
            let validation_errors = config.validate();
            if validation_errors.is_empty() {
                vm_success!("{}", MESSAGES.vm_doctor_config_valid);
            } else {
                vm_error!("{}", MESSAGES.vm_doctor_config_invalid);
                for error in validation_errors {
                    vm_println!("   ❌ {}", error);
                }
                config_ok = false;
            }

            // Check if configuration is complete
            if config.is_partial() {
                vm_error!("{}", MESSAGES.vm_doctor_config_incomplete);
                config_ok = false;
            } else {
                vm_success!("{}", MESSAGES.vm_doctor_config_complete);
            }
        }
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("No vm.yaml found") {
                vm_error!("{}", MESSAGES.vm_doctor_config_not_found);
                vm_println!("{}", MESSAGES.vm_doctor_config_not_found_hint);
            } else {
                vm_error!(
                    "{}",
                    msg!(MESSAGES.vm_doctor_config_load_failed, error = e.to_string())
                );
            }
            config_ok = false;
        }
    }

    config_ok
}

/// Check for required system dependencies
async fn check_system_dependencies() -> bool {
    let mut deps_ok = true;

    // Check for Docker
    match check_command_exists("docker") {
        Ok(true) => vm_success!("{}", MESSAGES.vm_doctor_docker_found),
        Ok(false) => {
            vm_error!("{}", MESSAGES.vm_doctor_docker_not_found);
            vm_println!("{}", MESSAGES.vm_doctor_docker_not_found_hint);
            deps_ok = false;
        }
        Err(e) => {
            vm_error!(
                "{}",
                msg!(
                    MESSAGES.vm_doctor_docker_check_failed,
                    error = e.to_string()
                )
            );
            deps_ok = false;
        }
    }

    // Check Docker daemon status (if Docker exists)
    if check_command_exists("docker").unwrap_or(false) {
        match check_docker_daemon() {
            Ok(true) => vm_success!("{}", MESSAGES.vm_doctor_docker_daemon_running),
            Ok(false) => {
                vm_error!("{}", MESSAGES.vm_doctor_docker_daemon_not_running);
                vm_println!("{}", MESSAGES.vm_doctor_docker_daemon_not_running_hint);
                deps_ok = false;
            }
            Err(e) => {
                vm_error!(
                    "{}",
                    msg!(
                        MESSAGES.vm_doctor_docker_daemon_check_failed,
                        error = e.to_string()
                    )
                );
                deps_ok = false;
            }
        }
    }

    // Check for git (commonly needed)
    match check_command_exists("git") {
        Ok(true) => vm_success!("{}", MESSAGES.vm_doctor_git_found),
        Ok(false) => {
            vm_error!("{}", MESSAGES.vm_doctor_git_not_found);
            vm_println!("{}", MESSAGES.vm_doctor_git_not_found_hint);
            deps_ok = false;
        }
        Err(e) => {
            vm_error!(
                "{}",
                msg!(MESSAGES.vm_doctor_git_check_failed, error = e.to_string())
            );
            deps_ok = false;
        }
    }

    deps_ok
}

/// Check the health of background services
async fn check_background_services(global_config: &GlobalConfig) -> bool {
    let mut services_ok = true;

    // Check auth proxy service
    let auth_url = format!(
        "http://127.0.0.1:{}/health",
        global_config.services.auth_proxy.port
    );
    match check_service_health(&auth_url).await {
        Ok(true) => vm_success!("{}", MESSAGES.vm_doctor_auth_healthy),
        Ok(false) => {
            vm_error!("{}", MESSAGES.vm_doctor_auth_not_responding);
            vm_println!("{}", MESSAGES.vm_doctor_auth_not_responding_hint);
            services_ok = false;
        }
        Err(e) => {
            vm_error!(
                "{}",
                msg!(MESSAGES.vm_doctor_auth_check_failed, error = e.to_string())
            );
            services_ok = false;
        }
    }

    // Check package server service
    let pkg_url = format!(
        "http://127.0.0.1:{}/health",
        global_config.services.package_registry.port
    );
    match check_service_health(&pkg_url).await {
        Ok(true) => vm_success!("{}", MESSAGES.vm_doctor_pkg_healthy),
        Ok(false) => {
            vm_error!("{}", MESSAGES.vm_doctor_pkg_not_responding);
            vm_println!("{}", MESSAGES.vm_doctor_pkg_not_responding_hint);
            services_ok = false;
        }
        Err(e) => {
            vm_error!(
                "{}",
                msg!(MESSAGES.vm_doctor_pkg_check_failed, error = e.to_string())
            );
            services_ok = false;
        }
    }

    // Check Docker registry service (silent unless there are active VMs)
    let docker_url = format!(
        "http://127.0.0.1:{}/v2/",
        global_config.services.docker_registry.port
    );
    match check_service_health(&docker_url).await {
        Ok(true) => vm_success!("{}", MESSAGES.vm_doctor_registry_healthy),
        Ok(false) => {
            // Only warn if there are active VMs that would benefit from the registry
            if has_active_vms().await {
                vm_error!("{}", MESSAGES.vm_doctor_registry_not_responding_active);
                vm_println!("{}", MESSAGES.vm_doctor_registry_not_responding_hint);
                services_ok = false;
            } else {
                // Silent check - registry not needed without active VMs
                vm_println!("{}", MESSAGES.vm_doctor_registry_not_running_info);
            }
        }
        Err(e) => {
            // Only treat as error if there are active VMs
            if has_active_vms().await {
                vm_error!(
                    "{}",
                    msg!(
                        MESSAGES.vm_doctor_registry_check_failed_active,
                        error = e.to_string()
                    )
                );
                services_ok = false;
            } else {
                vm_println!("{}", MESSAGES.vm_doctor_registry_check_skipped);
            }
        }
    }

    services_ok
}

/// Check if a command exists in the system PATH
fn check_command_exists(command: &str) -> Result<bool, std::io::Error> {
    let output = if cfg!(target_os = "windows") {
        Command::new("where").arg(command).output()
    } else {
        Command::new("which").arg(command).output()
    };

    match output {
        Ok(result) => Ok(result.status.success()),
        Err(e) => Err(e),
    }
}

/// Check if Docker daemon is running
fn check_docker_daemon() -> Result<bool, std::io::Error> {
    let output = Command::new("docker").args(["info"]).output()?;
    Ok(output.status.success())
}

/// Check if a service is healthy by making an HTTP request
async fn check_service_health(url: &str) -> Result<bool, Box<dyn std::error::Error>> {
    // Create a reqwest client with a short timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()?;

    match client.get(url).send().await {
        Ok(response) => Ok(response.status().is_success()),
        Err(_) => Ok(false), // Service not responding
    }
}

/// Check if there are any active VMs across all providers
async fn has_active_vms() -> bool {
    // Try to check for running Docker containers that look like VMs
    match Command::new("docker")
        .args(["ps", "--format", "{{.Names}}", "--filter", "status=running"])
        .output()
    {
        Ok(output) if output.status.success() => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // Look for common VM container patterns
            output_str
                .lines()
                .any(|line| line.contains("vm-") || line.contains("dev") || line.contains("temp"))
        }
        _ => false, // If we can't check, assume no active VMs
    }
}

/// Checks for legacy configuration file paths.
fn check_legacy_config_paths() -> bool {
    let mut legacy_paths_found: Vec<PathBuf> = Vec::new();

    // These paths must be constructed manually to avoid the fallback logic
    // in the user_paths functions, which is what we're trying to detect.
    let paths_to_check = [
        (
            user_paths::user_config_dir()
                .ok()
                .map(|p| p.join("global.yaml")),
            user_paths::vm_state_dir()
                .ok()
                .map(|p| p.join("config.yaml")),
        ),
        (
            user_paths::vm_state_dir()
                .ok()
                .map(|p| p.join("port-registry.json")),
            user_paths::vm_state_dir()
                .ok()
                .map(|p| p.join("ports.json")),
        ),
        (
            user_paths::vm_state_dir()
                .ok()
                .map(|p| p.join("service_state.json")),
            user_paths::vm_state_dir()
                .ok()
                .map(|p| p.join("services.json")),
        ),
        (
            user_paths::vm_state_dir()
                .ok()
                .map(|p| p.join("temp-vm.state")),
            user_paths::vm_state_dir()
                .ok()
                .map(|p| p.join("temp-vms.json")),
        ),
    ];

    for (old_path_opt, new_path_opt) in &paths_to_check {
        if let (Some(old_path), Some(new_path)) = (old_path_opt, new_path_opt) {
            if old_path.exists() && !new_path.exists() {
                legacy_paths_found.push(old_path.clone());
            }
        }
    }

    if legacy_paths_found.is_empty() {
        vm_success!("Using modern config paths");
        return true;
    }

    vm_error!("Old configuration files detected. Run 'vm config migrate' to update.");
    for path in legacy_paths_found {
        vm_println!("  - Legacy file needs migration: {}", path.display());
    }
    false
}
