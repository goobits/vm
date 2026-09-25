#[derive(Debug, Clone, Default, clap::Args)]
pub struct FleetArgs {
    /// Apply the command across matching managed environments
    #[arg(long = "all-envs")]
    pub fleet: bool,
    /// Provider filter (docker, podman, tart)
    #[arg(
        long = "match-provider",
        requires = "fleet",
        value_parser = vm_config::config::ProviderName::SUPPORTED
    )]
    pub provider: Option<String>,
    /// Match pattern for instance names
    #[arg(long = "match", requires = "fleet")]
    pub pattern: Option<String>,
}
