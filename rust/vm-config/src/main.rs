use clap::Parser;
use vm_config::cli::Args;
use vm_core::error::Result;

fn main() -> Result<()> {
    let args = Args::parse();
    vm_config::cli::execute(args)
}
