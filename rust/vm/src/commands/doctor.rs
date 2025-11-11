use anyhow::Result;
use std::process::Command;
use vm_core::{error::VmError, vm_error, vm_println, vm_success};
use vm_provider::docker::validate_docker_environment;

pub fn run() -> Result<()> {
    vm_println!("🔍 Running diagnostics...\n");
    let mut all_ok = true;

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
            if let VmError::DockerNotInstalled(_) = e {
                vm_error!("\nDocker is not installed.");
                vm_println!("  Please install Docker from https://docs.docker.com/get-docker/");
            } else if let VmError::DockerNotRunning(_) = e {
                vm_error!("\nDocker is not running.");
                vm_println!("  Please start Docker Desktop or run: sudo systemctl start docker");
            } else if let VmError::DockerPermission(_) = e {
                vm_error!("\nDocker permission denied.");
                vm_println!("  Your user does not have permission to access the Docker socket.");
                vm_println!("  Run the following command to add your user to the 'docker' group:");
                vm_println!("\n    sudo usermod -aG docker $USER && newgrp docker\n");
                vm_println!(
                    "  IMPORTANT: You may need to log out and log back in for this change to take effect."
                );
            } else {
                return Err(e.into());
            }
        }
    }

    // Check VM binary (implicit - we're running it)
    print!("  VM binary... ");
    println!("✓");

    if all_ok {
        vm_success!("\n✅ All checks passed! VM tool is ready.");
    } else {
        vm_error!("\n❌ Some checks failed. Please address the issues above.");
    }

    Ok(())
}
