use anyhow::Result;
use vm_common::{vm_error, vm_println, vm_success};
use which::which;

pub fn check() -> Result<()> {
    vm_println!("🔍 Checking dependencies...");
    if which("cargo").is_err() || which("rustc").is_err() {
        vm_error!("Rust/Cargo is not installed or not in your PATH.\nPlease install the Rust toolchain from https://rustup.rs to continue.");
        return Err(anyhow::anyhow!("Rust/Cargo not installed"));
    }
    vm_success!("Dependencies satisfied");
    Ok(())
}
