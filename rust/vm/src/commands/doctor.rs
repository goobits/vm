//! Doctor command handler
//!
//! This module provides diagnostic functionality to check the health and
//! configuration of the VM environment, including system dependencies,
//! configuration validation, and background service status.

use crate::error::{VmError, VmResult};
// use std::path::PathBuf; // Currently unused
use std::process::Command;
use vm_config::{config::VmConfig, GlobalConfig};
use vm_core::{vm_error, vm_println, vm_success};

/// Handle the doctor command - perform comprehensive health checks
pub async fn handle_doctor_command(global_config: GlobalConfig) -> VmResult<()> {
    vm_println!("🩺 VM Environment Health Check");
    vm_println!("==============================\n");

    let mut all_checks_passed = true;

    // 1. Configuration validation
    vm_println!("📋 Configuration Validation:");
    if !check_configuration().await {
        all_checks_passed = false;
    }
    vm_println!();

    // 2. System dependencies
    vm_println!("🔧 System Dependencies:");
    if !check_system_dependencies().await {
        all_checks_passed = false;
    }
    vm_println!();

    // 3. Background services
    vm_println!("🔄 Background Services:");
    if !check_background_services(&global_config).await {
        all_checks_passed = false;
    }
    vm_println!();

    // Final summary
    vm_println!("==============================");
    if all_checks_passed {
        vm_success!("All checks passed! Your VM environment is healthy.");
    } else {
        vm_error!("Some checks failed. Please review the issues above.");
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
            vm_success!("Configuration loaded successfully");

            // Run validation
            let validation_errors = config.validate();
            if validation_errors.is_empty() {
                vm_success!("Configuration validation passed");
            } else {
                vm_error!("Configuration validation failed:");
                for error in validation_errors {
                    vm_println!("   ❌ {}", error);
                }
                config_ok = false;
            }

            // Check if configuration is complete
            if config.is_partial() {
                vm_error!("Configuration is incomplete (missing provider or project name)");
                config_ok = false;
            } else {
                vm_success!("Configuration is complete");
            }
        }
        Err(e) => {
            let error_str = e.to_string();
            if error_str.contains("No vm.yaml found") {
                vm_error!("No vm.yaml configuration file found");
                vm_println!("   💡 Run 'vm init' to create a configuration file");
            } else {
                vm_error!("Failed to load configuration: {}", e);
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
        Ok(true) => vm_success!("Docker command found"),
        Ok(false) => {
            vm_error!("Docker command not found in PATH");
            vm_println!("   💡 Install Docker: https://docs.docker.com/get-docker/");
            deps_ok = false;
        }
        Err(e) => {
            vm_error!("Failed to check Docker: {}", e);
            deps_ok = false;
        }
    }

    // Check Docker daemon status (if Docker exists)
    if check_command_exists("docker").unwrap_or(false) {
        match check_docker_daemon() {
            Ok(true) => vm_success!("Docker daemon is running"),
            Ok(false) => {
                vm_error!("Docker daemon is not running");
                vm_println!("   💡 Start Docker daemon or Docker Desktop");
                deps_ok = false;
            }
            Err(e) => {
                vm_error!("Failed to check Docker daemon: {}", e);
                deps_ok = false;
            }
        }
    }

    // Check for git (commonly needed)
    match check_command_exists("git") {
        Ok(true) => vm_success!("Git command found"),
        Ok(false) => {
            vm_error!("Git command not found in PATH");
            vm_println!("   💡 Install Git for version control support");
            deps_ok = false;
        }
        Err(e) => {
            vm_error!("Failed to check Git: {}", e);
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
        Ok(true) => vm_success!("Auth proxy service is healthy"),
        Ok(false) => {
            vm_error!("Auth proxy service is not responding");
            vm_println!("   💡 Start with: vm auth start");
            services_ok = false;
        }
        Err(e) => {
            vm_error!("Failed to check auth proxy: {}", e);
            services_ok = false;
        }
    }

    // Check package server service
    let pkg_url = format!(
        "http://127.0.0.1:{}/health",
        global_config.services.package_registry.port
    );
    match check_service_health(&pkg_url).await {
        Ok(true) => vm_success!("Package server service is healthy"),
        Ok(false) => {
            vm_error!("Package server service is not responding");
            vm_println!("   💡 Start with: vm pkg start");
            services_ok = false;
        }
        Err(e) => {
            vm_error!("Failed to check package server: {}", e);
            services_ok = false;
        }
    }

    // Check Docker registry service (silent unless there are active VMs)
    let docker_url = format!(
        "http://127.0.0.1:{}/v2/",
        global_config.services.docker_registry.port
    );
    match check_service_health(&docker_url).await {
        Ok(true) => vm_success!("Docker registry service is healthy"),
        Ok(false) => {
            // Only warn if there are active VMs that would benefit from the registry
            if has_active_vms().await {
                vm_error!("Docker registry service is not responding (needed for active VMs)");
                vm_println!("   💡 Registry helps cache Docker images for faster VM operations");
                services_ok = false;
            } else {
                // Silent check - registry not needed without active VMs
                vm_println!(
                    "   ℹ️  Docker registry service not running (not needed without active VMs)"
                );
            }
        }
        Err(e) => {
            // Only treat as error if there are active VMs
            if has_active_vms().await {
                vm_error!("Failed to check Docker registry: {}", e);
                services_ok = false;
            } else {
                vm_println!("   ℹ️  Docker registry check skipped (not needed without active VMs)");
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
