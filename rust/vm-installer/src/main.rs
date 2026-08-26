use clap::Parser;
use vm_core::error::Result;
use vm_core::{vm_error, vm_println, vm_success};
use vm_installer::{check_dependencies, install};
use vm_logging::init_subscriber;
use vm_messages::messages::MESSAGES;

mod cli;

use cli::Args;

fn main() {
    if let Err(e) = run() {
        vm_error!("{:#}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    // The guard must be kept in scope for the lifetime of the application
    // to ensure that all buffered logs are flushed to the file.
    let _guard = init_subscriber();
    let span = tracing::info_span!("installer", component = "vm_installer");
    let _enter = span.enter();
    let args = Args::parse();

    vm_println!("{}", MESSAGES.service.installer_installing);

    // 1. Check dependencies (like cargo)
    check_dependencies()?;

    // 2. Run the installation
    install(args.clean)?;

    vm_success!("Installation complete!");
    vm_println!("{}", MESSAGES.service.installer_complete);
    vm_println!("{}", MESSAGES.service.installer_help_hint);
    Ok(())
}
