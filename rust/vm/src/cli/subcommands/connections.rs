use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Clone, Subcommand)]
pub enum TunnelSubcommand {
    /// Open a named loopback tunnel to a port in an environment
    Open {
        name: String,
        #[arg(long)]
        local: String,
        #[arg(long)]
        remote: String,
        #[arg(long)]
        env: Option<String>,
    },
    /// List active tunnels
    List {
        #[arg(long)]
        env: Option<String>,
    },
    /// Close one named tunnel
    Close {
        name: String,
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SecretSubcommand {
    /// Check secret proxy status
    Status,
    /// Store a secret
    Set {
        name: String,
        #[arg(long, conflicts_with = "file")]
        stdin: bool,
        #[arg(long, conflicts_with = "stdin")]
        file: Option<PathBuf>,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// See all secrets
    List,
    /// Reveal one secret value
    Show {
        name: String,
        #[arg(long, required = true)]
        reveal: bool,
    },
    /// Delete a secret
    Remove {
        name: String,
        /// Confirm removal without prompting
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DbSubcommand {
    /// Manage PostgreSQL backups
    Backups {
        #[command(subcommand)]
        command: DbBackupSubcommand,
    },
    /// List databases
    List {
        #[arg(long)]
        env: Option<String>,
    },
    /// Show the size and backup count of a database
    Status {
        name: String,
        #[arg(long)]
        env: Option<String>,
    },
    /// Export a database to a SQL file
    Export {
        name: String,
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        overwrite: bool,
        #[arg(long)]
        env: Option<String>,
    },
    /// Import a database from a SQL file
    Import {
        name: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        env: Option<String>,
    },
    /// Drop and recreate a database
    Reset {
        name: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        env: Option<String>,
    },
    /// Show credentials metadata, or reveal the value explicitly
    Credentials {
        service: String,
        #[arg(long)]
        reveal: bool,
        #[arg(long)]
        env: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DbBackupSubcommand {
    /// List retained backups
    List {
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        env: Option<String>,
    },
    /// Create a backup for one database or all databases
    Create {
        name: String,
        #[arg(long, conflicts_with = "all", required_unless_present = "all")]
        database: Option<String>,
        #[arg(long, conflicts_with = "database")]
        all: bool,
        #[arg(long)]
        env: Option<String>,
    },
    /// Restore a backup into one database
    Restore {
        backup: String,
        #[arg(long)]
        database: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        env: Option<String>,
    },
    /// Remove one retained backup
    Remove {
        backup: String,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        env: Option<String>,
    },
}
