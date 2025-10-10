use clap::Parser;
use vm_config::cli::Args;
use vm_core::error::Result;

fn main() -> Result<()> {
    // We don't have a --verbose flag here, so we default to false
    vm_logging::init_subscriber_with_config(false);
    let args = Args::parse();
    vm_config::cli::execute(args)
}
