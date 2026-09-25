use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Clone, Subcommand)]
pub enum SnapshotSubcommand {
    /// List snapshots for the selected environment
    List {
        #[arg(long)]
        env: Option<String>,
    },
    /// Show snapshot metadata
    Show {
        name: String,
        #[arg(long)]
        env: Option<String>,
    },
    /// Capture an environment
    Create {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        quiesce: bool,
    },
    /// Restore a snapshot into an environment
    Restore {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Delete a snapshot
    Remove {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        yes: bool,
    },
    /// Export a snapshot as a portable archive
    Export {
        name: String,
        #[arg(long)]
        env: Option<String>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, default_value_t = 6)]
        compression: u8,
        /// Replace an existing archive
        #[arg(long)]
        overwrite: bool,
    },
    /// Import a portable snapshot archive
    Import {
        archive: PathBuf,
        #[arg(long)]
        name: String,
    },
}
