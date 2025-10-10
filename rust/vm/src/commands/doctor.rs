use anyhow::Result;
use vm_core::{vm_println, vm_success};
use vm_provider::docker::validate_docker_environment;

pub fn run() -> Result<()> {
    vm_println!("🔍 Running diagnostics...\n");

    validate_docker_environment()?;

    vm_success!("\n✅ All checks passed! VM tool is ready.");
    Ok(())
}
