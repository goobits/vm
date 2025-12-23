//! VM Doctor command - Health checks and auto-fixes
//!
//! This module provides diagnostic checks for the VM tool and
//! optionally attempts to fix common issues.

use anyhow::Result;
use std::process::Command;
use vm_core::{vm_error, vm_println, vm_success};
use vm_provider::docker::validate_docker_environment;

/// Run diagnostics without attempting fixes
#[allow(dead_code)]
pub fn run() -> Result<()> {
    run_diagnostics(false)
}

/// Run diagnostics with optional auto-fix
pub fn run_with_fix(fix: bool) -> Result<()> {
    run_diagnostics(fix)
}

/// Internal diagnostic runner
fn run_diagnostics(fix: bool) -> Result<()> {
    vm_println!("🔍 Running diagnostics...\n");
    let mut all_ok = true;
    let mut issues_fixed = 0;

    // Check Rust installation
    print!("  Rust installed... ");
    if Command::new("rustc").arg("--version").status().is_ok() {
        println!("✓");
    } else {
        println!("⚠️  (not required, but needed for `cargo install goobits-vm`)");
    }

    // Check Docker (critical)
    print!("  Docker environment... ");
    match validate_docker_environment() {
        Ok(_) => {
            println!("✓");
        }
        Err(e) => {
            all_ok = false;
            println!("❌");

            let error_str = e.to_string();
            if error_str.contains("not installed") {
                vm_error!("\nDocker is not installed.");
                vm_println!("  Please install Docker from https://docs.docker.com/get-docker/");
            } else if error_str.contains("not running") {
                vm_error!("\nDocker is not running.");
                if fix {
                    if try_start_docker() {
                        vm_success!("  ✓ Docker started successfully");
                        issues_fixed += 1;
                        all_ok = true;
                    } else {
                        vm_println!(
                            "  Please start Docker Desktop or run: sudo systemctl start docker"
                        );
                    }
                } else {
                    vm_println!(
                        "  Please start Docker Desktop or run: sudo systemctl start docker"
                    );
                    vm_println!("  💡 Or run: vm doctor --fix");
                }
            } else if error_str.contains("permission") {
                vm_error!("\nDocker permission denied.");
                vm_println!("  Your user does not have permission to access the Docker socket.");
                if fix {
                    if try_fix_docker_permissions() {
                        vm_success!("  ✓ Added user to docker group");
                        vm_println!("  ⚠️  You may need to log out and log back in for this change to take effect.");
                        issues_fixed += 1;
                    } else {
                        vm_println!(
                            "  Run the following command to add your user to the 'docker' group:"
                        );
                        vm_println!("\n    sudo usermod -aG docker $USER && newgrp docker\n");
                    }
                } else {
                    vm_println!(
                        "  Run the following command to add your user to the 'docker' group:"
                    );
                    vm_println!("\n    sudo usermod -aG docker $USER && newgrp docker\n");
                    vm_println!("  💡 Or run: vm doctor --fix");
                }
            } else {
                return Err(e.into());
            }
        }
    }

    // Check SSH key permissions
    print!("  SSH key permissions... ");
    match check_ssh_permissions() {
        Ok(_) => {
            println!("✓");
        }
        Err(msg) => {
            all_ok = false;
            println!("⚠️");
            vm_println!("  {}", msg);
            if fix {
                if try_fix_ssh_permissions() {
                    vm_success!("  ✓ Fixed SSH key permissions");
                    issues_fixed += 1;
                    all_ok = true;
                }
            } else {
                vm_println!("  💡 Run: vm doctor --fix");
            }
        }
    }

    // Check common port availability
    print!("  Common ports available... ");
    let port_conflicts = check_port_conflicts();
    if port_conflicts.is_empty() {
        println!("✓");
    } else {
        println!("⚠️");
        vm_println!("  Ports in use: {:?}", port_conflicts);
        vm_println!("  These ports may conflict with VM services");
        // We don't auto-fix port conflicts as it could kill user processes
    }

    // Check VM binary (implicit - we're running it)
    print!("  VM binary... ");
    println!("✓");

    // Check vm config directory
    print!("  Config directory... ");
    match check_config_directory() {
        Ok(_) => {
            println!("✓");
        }
        Err(msg) => {
            println!("⚠️");
            vm_println!("  {}", msg);
            if fix && try_create_config_directory() {
                vm_success!("  ✓ Created config directory");
                issues_fixed += 1;
            }
        }
    }

    // Summary
    println!();
    if all_ok {
        vm_success!("✅ All checks passed! VM tool is ready.");
    } else if fix && issues_fixed > 0 {
        vm_success!("✅ Fixed {} issue(s)!", issues_fixed);
        vm_println!("   Some issues may require logging out and back in.");
    } else {
        vm_error!("❌ Some checks failed. Please address the issues above.");
        if !fix {
            vm_println!("\n💡 Tip: Run 'vm doctor --fix' to attempt automatic fixes");
        }
    }

    Ok(())
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
                return validate_docker_environment().is_ok();
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
                return validate_docker_environment().is_ok();
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
