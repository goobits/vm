use clap::Parser;
use vm_config::cli::Args;
use vm_core::error::Result;
use vm_logging::init_subscriber;

fn main() -> Result<()> {
    // The guard must be kept in scope for the lifetime of the application
    // to ensure that all buffered logs are flushed to the file.
    let _guard = init_subscriber();
    let args = Args::parse();
    vm_config::cli::execute(args)
}
