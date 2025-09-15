use anyhow::Result;
use clap::Parser;
use vm_config::cli::Args;

fn main() -> Result<()> {
    let args = Args::parse();
    vm_config::cli::execute(args)
}
