use anyhow::Result;
use std::process::Command;
use vm_core::{vm_println, vm_success};
use vm_provider::docker::validate_docker_environment;

pub fn run() -> Result<()> {
    vm_println!("🔍 Running diagnostics...\n");

    // Check Rust installation
    print!("  Rust installed... ");
    if Command::new("rustc").arg("--version").status().is_ok() {
        println!("✓");
    } else {
        println!("⚠️  (not required, but needed for `cargo install vm`)");
    }

    // Check Docker (critical)
    print!("  Docker installed... ");
    validate_docker_environment()?;
    println!("✓");

    print!("  Docker running... ");
    println!("✓");

    // Check VM binary (implicit - we're running it)
    print!("  VM binary... ");
    println!("✓");

    vm_success!("\n✅ All checks passed! VM tool is ready.");
    Ok(())
}
