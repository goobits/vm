use anyhow::{bail, Result};
use vm_common::{vm_println, vm_success};
use which::which;

pub fn check() -> Result<()> {
    vm_println!("🔍 Checking dependencies...");
    if which("cargo").is_err() || which("rustc").is_err() {
        bail!(
            "Rust/Cargo is not installed or not in your PATH.\nPlease install the Rust toolchain from https://rustup.rs to continue."
        );
    }
    vm_success!("Dependencies satisfied");
    Ok(())
}
