use anyhow::Result;
use clap::Parser;
use colored::*;

mod cli;
mod dependencies;
mod installer;
mod platform;

use cli::Args;
use installer::install;

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {:#}", "Error:".red().bold(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();

    println!("{}", "🚀 Installing VM Infrastructure...".bold());

    // 1. Check dependencies (like cargo)
    dependencies::check()?;

    // 2. Run the installation
    install(args.clean)?;

    println!("\n{}", "🎉 Installation complete!".green().bold());
    println!("\nThe 'vm' command is now available in new terminal sessions.");
    println!("For more information, run: vm --help");
    Ok(())
}
