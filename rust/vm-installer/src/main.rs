use clap::Parser;
use vm_core::error::Result;
use vm_core::{vm_error, vm_println, vm_success};
use vm_messages::messages::MESSAGES;

mod cli;
mod dependencies;
mod installer;
mod platform;
mod prompt;

use cli::Args;
use installer::install;

fn main() {
    if let Err(e) = run() {
        vm_error!("{:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    vm_println!("{}", MESSAGES.installer_installing);

    // 1. Check dependencies (like cargo)
    dependencies::check()?;

    // 2. Run the installation
    install(args.clean)?;

    vm_success!("Installation complete!");
    vm_println!("{}", MESSAGES.installer_complete);
    vm_println!("{}", MESSAGES.installer_help_hint);
    Ok(())
}
