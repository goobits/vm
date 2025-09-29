mod cli;
mod installer;
mod link_detector;
mod links;
mod package_manager;

use clap::Parser;
use vm_core::error::Result;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    cli::execute(args)
}
