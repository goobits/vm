use clap::Parser;
use vm_config::cli::Args;
use vm_core::error::Result;

fn main() -> Result<()> {
    vm_logging::init_subscriber();
    let args = Args::parse();
    vm_config::cli::execute(args)
}
