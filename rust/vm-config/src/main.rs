mod config;
mod merge;
mod preset;
mod validate;
mod cli;
mod yaml_ops;

use anyhow::Result;
use clap::Parser;
use cli::Args;

fn main() -> Result<()> {
    let args = Args::parse();
    cli::execute(args)
}
