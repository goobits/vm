use anyhow::Result;
use vm_config::cli::Args;
use clap::Parser;

fn main() -> Result<()> {
    let args = Args::parse();
    vm_config::cli::execute(args)
}
