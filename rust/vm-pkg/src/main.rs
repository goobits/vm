mod cli;
mod installer;
mod link_detector;
mod package_manager;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let args = cli::Args::parse();
    cli::execute(args)
}