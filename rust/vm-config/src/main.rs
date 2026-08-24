use clap::Parser;
use vm_config::CliArgs;
use vm_core::error::Result;
use vm_logging::init_subscriber;

fn main() -> Result<()> {
    // The guard must be kept in scope for the lifetime of the application
    // to ensure that all buffered logs are flushed to the file.
    let _guard = init_subscriber();
    let args = CliArgs::parse();
    vm_config::execute_cli(args)
}
