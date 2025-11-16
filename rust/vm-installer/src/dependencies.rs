use vm_core::{error::Result, vm_error, vm_println, vm_success};
use vm_messages::messages::MESSAGES;

pub fn check() -> Result<()> {
    vm_println!("{}", MESSAGES.service.installer_checking_dependencies);
    use std::process::Command;

    let cargo_check = Command::new("cargo").arg("--version").output();
    let rustc_check = Command::new("rustc").arg("--version").output();

    if cargo_check.is_err() || rustc_check.is_err() {
        vm_error!("Rust/Cargo is not installed or not in your PATH.\nPlease install the Rust toolchain from https://rustup.rs to continue.");
        return Err(vm_core::error::VmError::Internal(
            "Rust/Cargo not installed".to_string(),
        ));
    }
    vm_success!("Dependencies satisfied");
    Ok(())
}
