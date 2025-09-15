use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "vm-temp")]
#[command(about = "Temporary VM management for VM Tool")]
#[command(version)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create temp VM with mounts
    Create {
        /// Directories to mount (e.g., ./src,./config:ro)
        mounts: Vec<String>,

        /// Auto-destroy on exit
        #[arg(long)]
        auto_destroy: bool,
    },
    /// SSH into temp VM
    Ssh,
    /// Show temp VM status
    Status,
    /// Destroy temp VM
    Destroy,
    /// Add mount to running temp VM
    Mount {
        /// Path to mount (e.g., ./src or ./config:ro)
        path: String,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Remove mount from temp VM
    Unmount {
        /// Path to unmount (omit for --all)
        path: Option<String>,
        /// Remove all mounts
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// List current mounts
    Mounts,
    /// List all temp VMs
    List,
    /// Stop temp VM
    Stop,
    /// Start temp VM
    Start,
    /// Restart temp VM
    Restart,
}
