use clap::Subcommand;

#[derive(Debug, Clone, Subcommand)]
pub enum ToolsSubcommand {
    #[command(hide = true)]
    ActivationWorker {
        #[arg(long)]
        once: bool,
    },
    #[command(hide = true)]
    ReconcileWorker { environment: String },
    /// Register one trusted tool source with package infrastructure
    Register {
        name: String,
        #[arg(long)]
        repository: String,
        #[arg(long, default_value = "main")]
        branch: String,
        #[arg(long, default_value = "binary", value_parser = ["binary", "collection"])]
        kind: String,
    },
    /// List VM-owned vendor tools and registered package tools
    List,
    /// Show one vendor definition or package tool and its published releases
    Show { name: String },
    /// Remove a managed tool registration while preserving published artifacts
    Remove { name: String },
    /// Refresh the appliance-generated tool catalog cache
    Refresh {
        /// Refresh these tools' selected releases (omit to refresh all selected tools)
        #[arg(value_name = "NAME")]
        names: Vec<String>,
        #[arg(long, hide = true)]
        quiet: bool,
    },
    /// Show vendor, registered, published, installed, and consumable tool state
    Status {
        #[arg(long)]
        env: Option<String>,
    },
    /// Select package tools globally and activate them in running managed Docker environments
    Enable {
        #[arg(required = true, value_name = "TOOL")]
        tools: Vec<String>,
    },
    /// Stop selecting package tools globally; existing managed files are retained
    Disable {
        #[arg(required = true, value_name = "TOOL")]
        tools: Vec<String>,
    },
    /// Update VM-owned vendor tools and configured package tools across managed environments
    Update {
        /// Vendor or package tool names to filter; omit to update all eligible tools
        #[arg(value_name = "TOOL")]
        tools: Vec<String>,
        /// Update only these project environments
        #[arg(long, value_name = "ENVIRONMENT", action = clap::ArgAction::Append, conflicts_with = "all_envs")]
        env: Vec<String>,
        /// Update every running environment in this project
        #[arg(long, conflicts_with = "env")]
        all_envs: bool,
        /// Record updates for stopped environments without starting them
        #[arg(long)]
        include_stopped: bool,
        /// Reconcile prerequisites, then return after launching tool updates
        #[arg(long)]
        background: bool,
    },
}
