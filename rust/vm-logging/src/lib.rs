use tracing_subscriber::EnvFilter;

pub fn init_subscriber_with_config(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("vm=debug")
    } else {
        EnvFilter::new("vm=warn") // Only errors by default
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(verbose) // Hide targets unless verbose
        .with_level(verbose)   // Hide levels unless verbose
        .init();
}
