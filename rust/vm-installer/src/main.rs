use anyhow::Result;
use clap::Parser;
use vm_common::{vm_error, vm_println, vm_success};

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

    vm_println!("Installing VM Infrastructure...");

    // 1. Check dependencies (like cargo)
    dependencies::check()?;

    // 2. Run the installation
    install(args.clean)?;

    vm_success!("Installation complete!");
    vm_println!("The 'vm' command is now available in new terminal sessions.");
    vm_println!("For more information, run: vm --help");
    Ok(())
}
