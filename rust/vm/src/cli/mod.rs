// CLI argument parsing and definitions

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Parser)]
#[command(name = "vm")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "Goobits VM Contributors")]
#[command(about = "Humane virtual environments")]
#[command(before_help = format!(" \nvm v{}", env!("CARGO_PKG_VERSION")))]
#[command(after_help = " \nRun `vm help <command>` for specific options.\n")]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,

    /// Path to a custom VM configuration file
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Select a configuration profile to apply
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Show what would be executed without running
    #[arg(long, global = true)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, ValueEnum)]
pub enum EnvironmentKind {
    /// A macOS virtual machine powered by Tart
    Mac,
    /// A Linux development environment
    Linux,
    /// A generic container environment
    Container,
}

impl EnvironmentKind {
    pub fn default_provider(self) -> &'static str {
        match self {
            Self::Mac => "tart",
            Self::Linux | Self::Container => "docker",
        }
    }
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigSubcommand {
    /// Validate the current configuration
    Validate,
    /// Show the loaded configuration and its source
    Show,
    /// Change a configuration value
    Set {
        /// Configuration field path (e.g., "vm.memory" or "services.docker.enabled")
        field: String,
        /// Value(s) to set
        #[arg(required = true, num_args = 1..)]
        values: Vec<String>,
        /// Apply to global configuration
        #[arg(long)]
        global: bool,
    },
    /// View configuration values
    Get {
        /// Configuration field path (omit to show all)
        field: Option<String>,
        /// Read from global configuration
        #[arg(long)]
        global: bool,
    },
    /// Remove a configuration value
    Unset {
        /// Configuration field path to remove
        field: String,
        /// Remove from global configuration
        #[arg(long)]
        global: bool,
    },
    /// Add preset configurations
    Preset {
        /// Preset names (comma-separated for multiple)
        names: Option<String>,
        /// Apply to global configuration
        #[arg(long)]
        global: bool,
        /// List available presets
        #[arg(long)]
        list: bool,
        /// Show preset details
        #[arg(long)]
        show: Option<String>,
    },
    /// Manage configuration profiles
    Profile {
        #[command(subcommand)]
        command: ConfigProfileSubcommand,
    },
    /// Fix port conflicts
    Ports {
        /// Fix port conflicts automatically
        #[arg(long)]
        fix: bool,
    },
    /// Reset your configuration
    Clear {
        /// Clear global configuration instead of local
        #[arg(long)]
        global: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ConfigProfileSubcommand {
    /// List available profiles for this project
    Ls,
    /// Set the default profile for this project
    Set { name: String },
}

#[derive(Debug, Clone, clap::Args)]
pub struct FleetTargetArgs {
    /// Provider filter (docker, podman, tart)
    #[arg(long)]
    pub provider: Option<String>,
    /// Match pattern for instance names
    #[arg(long)]
    pub pattern: Option<String>,
    /// Only include running instances
    #[arg(long)]
    pub running: bool,
    /// Only include stopped instances
    #[arg(long)]
    pub stopped: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum FleetSubcommand {
    /// List instances across providers
    Ls {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
    /// Show status for instances across providers
    Status {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
    /// Run a command across instances
    #[command(trailing_var_arg = true)]
    Exec {
        #[command(flatten)]
        targets: FleetTargetArgs,
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// Copy files to/from instances
    Copy {
        #[command(flatten)]
        targets: FleetTargetArgs,
        source: String,
        destination: String,
    },
    /// Start instances
    Start {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
    /// Stop instances
    Stop {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
    /// Restart instances
    Restart {
        #[command(flatten)]
        targets: FleetTargetArgs,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TunnelSubcommand {
    /// Add a tunnel (e.g., vm tunnel add 8080:3000 backend)
    Add {
        mapping: String,
        environment: Option<String>,
    },
    /// List active tunnels
    Ls { environment: Option<String> },
    /// Stop tunnel(s)
    Stop {
        port: Option<u16>,
        environment: Option<String>,
        #[arg(long)]
        all: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SecretSubcommand {
    /// Check secret proxy status
    Status,
    /// Store a secret
    Add {
        name: String,
        value: String,
        #[arg(long)]
        scope: Option<String>,
        #[arg(long)]
        description: Option<String>,
    },
    /// See all secrets
    Ls {
        #[arg(long)]
        show_values: bool,
    },
    /// Delete a secret
    Rm {
        name: String,
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Add a secret interactively
    Interactive,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DbSubcommand {
    /// Backup a database
    Backup {
        db_name: Option<String>,
        name: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Restore a database from a backup
    Restore { name: String, db_name: String },
    /// List all databases and backups
    Ls,
    /// Export a database to a SQL file
    Export { name: String, file: PathBuf },
    /// Import a database from a SQL file
    Import { file: PathBuf, db_name: String },
    /// Show disk usage per database
    Size,
    /// Drop and recreate a database
    Reset {
        name: String,
        #[arg(long)]
        force: bool,
    },
    /// Show credentials for a database service
    Credentials { service: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum RegistrySubcommand {
    /// Check registry server status
    Status {
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Publish a package to the registry
    Add {
        #[arg(long, short = 't')]
        r#type: Option<String>,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Remove a package from the registry
    Rm {
        #[arg(long, short = 'f')]
        force: bool,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// List packages in the registry
    Ls {
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Manage registry settings
    Config {
        #[command(subcommand)]
        action: RegistryConfigAction,
    },
    /// Get shell configuration for using the registry
    Use {
        #[arg(long)]
        shell: Option<String>,
        #[arg(long, default_value = "3080")]
        port: u16,
    },
    /// Start registry server
    #[command(hide = true)]
    Serve {
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        #[arg(long, default_value = "3080")]
        port: u16,
        #[arg(long)]
        data: PathBuf,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum RegistryConfigAction {
    Show,
    Get { key: String },
    Set { key: String, value: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum BaseSubcommand {
    /// Build a provider-native base artifact for a preset
    Build {
        preset: String,
        #[arg(long, value_parser = ["docker", "tart"])]
        provider: String,
    },
    /// Validate the shared provider workflow for the current project
    Validate {
        preset: String,
        #[arg(long, value_parser = ["docker", "tart", "all"], default_value = "all")]
        provider: String,
        #[arg(long)]
        rebuild_docker_base: bool,
        #[arg(long)]
        build_tart_base: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SystemSubcommand {
    /// Update this vm installation
    Update {
        #[arg(long)]
        version: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Remove vm from this system
    Uninstall {
        #[arg(long)]
        keep_config: bool,
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Manage package registries
    Registry {
        #[command(subcommand)]
        command: RegistrySubcommand,
    },
    /// Build and validate provider-native base environments
    Base {
        #[command(subcommand)]
        command: BaseSubcommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PluginSubcommand {
    /// See installed plugins
    Ls,
    /// Get plugin details
    Info { plugin_name: String },
    /// Add a plugin
    Install { source_path: String },
    /// Remove a plugin
    Rm { plugin_name: String },
    /// Create a new plugin
    New {
        plugin_name: String,
        #[arg(long)]
        r#type: String,
    },
    /// Check plugin configuration
    Validate { plugin_name: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Create and start an environment
    Run {
        /// Environment kind: mac, linux, or container
        kind: EnvironmentKind,
        /// Optional natural-language name: as <name>
        #[arg(num_args = 0..=2)]
        words: Vec<String>,
        /// Advanced provider override
        #[arg(long, value_parser = ["docker", "podman", "tart"])]
        provider: Option<String>,
        /// Use a specific image, distro, or snapshot name
        #[arg(long)]
        image: Option<String>,
        /// Build from a local Dockerfile or context
        #[arg(long, value_name = "PATH")]
        build: Option<PathBuf>,
        /// Clone from a saved snapshot
        #[arg(long = "from-snapshot")]
        from_snapshot: Option<String>,
        /// Remove when stopped/exited
        #[arg(long)]
        ephemeral: bool,
        /// Mount a local folder into the environment
        #[arg(long)]
        mount: Vec<String>,
        /// CPU limit
        #[arg(long)]
        cpu: Option<String>,
        /// Memory limit
        #[arg(long)]
        memory: Option<String>,
    },
    /// List environments for this project
    #[command(alias = "ls")]
    List {
        /// Show environments across all projects
        #[arg(long)]
        all: bool,
        /// Show provider IDs and raw provider names
        #[arg(long)]
        raw: bool,
    },
    /// Drop into a shell inside an environment
    Shell {
        environment: Option<String>,
        /// Directory path to start shell in
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Run a single command inside an environment
    #[command(trailing_var_arg = true)]
    Exec {
        environment: String,
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// Stream output logs from an environment
    Logs {
        environment: Option<String>,
        #[arg(short = 'f', long)]
        follow: bool,
        #[arg(short = 'n', long, default_value = "50")]
        tail: usize,
        #[arg(short = 's', long)]
        service: Option<String>,
    },
    /// Move files between host and environment
    Copy { source: String, destination: String },
    /// Gracefully halt an environment
    Stop { environment: Option<String> },
    /// Stop and start an environment
    Restart { environment: Option<String> },
    /// Remove an environment while preserving saved snapshots
    #[command(alias = "rm")]
    Remove {
        environment: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// Save the current state of an environment
    Save {
        /// Either `as <snapshot>` or `<environment> as <snapshot>`
        #[arg(required = true, num_args = 2..=3)]
        words: Vec<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        quiesce: bool,
        #[arg(long)]
        force: bool,
    },
    /// Restore an environment to a saved state
    Revert {
        /// Either `<snapshot>` or `<environment> <snapshot>`
        #[arg(required = true, num_args = 1..=2)]
        words: Vec<String>,
        #[arg(long)]
        force: bool,
    },
    /// Export an environment or base as a portable artifact
    Package {
        environment: Option<String>,
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
        #[arg(long, default_value = "6")]
        compress: u8,
        /// Build package directly from a Dockerfile
        #[arg(long, value_name = "PATH")]
        build: Option<PathBuf>,
    },
    /// Manage defaults, providers, and profiles
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },
    /// Manage active port forwards
    Tunnel {
        #[command(subcommand)]
        command: TunnelSubcommand,
    },
    /// Diagnose and repair engine issues
    Doctor {
        #[arg(long)]
        fix: bool,
        #[arg(long)]
        clean: bool,
    },
    /// Extend with plugins
    Plugin {
        #[command(subcommand)]
        command: PluginSubcommand,
    },
    /// Self-management and lower-level system tools
    System {
        #[command(subcommand)]
        command: SystemSubcommand,
    },
    /// Plugin-backed database workflows
    Db {
        #[command(subcommand)]
        command: DbSubcommand,
    },
    /// Plugin-backed fleet workflows
    Fleet {
        #[command(subcommand)]
        command: FleetSubcommand,
    },
    /// Plugin-backed secret workflows
    Secret {
        #[command(subcommand)]
        command: SecretSubcommand,
    },
    #[command(hide = true)]
    InternalCompletion { shell: String },
    /// Get workspace directory
    #[command(hide = true)]
    GetSyncDirectory,
}

#[cfg(test)]
mod tests {
    use super::{Args, Command, DbSubcommand, EnvironmentKind, PluginSubcommand, SystemSubcommand};
    use clap::Parser;

    #[test]
    fn run_parses_kind_and_humane_name() {
        let args = Args::parse_from(["vm", "run", "linux", "as", "backend"]);
        match args.command {
            Command::Run { kind, words, .. } => {
                assert_eq!(kind, EnvironmentKind::Linux);
                assert_eq!(words, vec!["as", "backend"]);
            }
            _ => panic!("Expected Command::Run"),
        }
    }

    #[test]
    fn shell_parses_environment() {
        let args = Args::parse_from(["vm", "shell", "backend"]);
        match args.command {
            Command::Shell { environment, .. } => assert_eq!(environment, Some("backend".into())),
            _ => panic!("Expected Command::Shell"),
        }
    }

    #[test]
    fn ls_parses_all_flag() {
        let args = Args::parse_from(["vm", "ls", "--all"]);
        match args.command {
            Command::List { all, raw } => {
                assert!(all);
                assert!(!raw);
            }
            _ => panic!("Expected Command::List"),
        }
    }

    #[test]
    fn list_parses_raw_flag() {
        let args = Args::parse_from(["vm", "list", "--raw"]);
        match args.command {
            Command::List { all, raw } => {
                assert!(!all);
                assert!(raw);
            }
            _ => panic!("Expected Command::List"),
        }
    }

    #[test]
    fn restart_parses_environment() {
        let args = Args::parse_from(["vm", "restart", "backend"]);
        match args.command {
            Command::Restart { environment } => assert_eq!(environment, Some("backend".into())),
            _ => panic!("Expected Command::Restart"),
        }
    }

    #[test]
    fn exec_parses_command() {
        let args = Args::parse_from(["vm", "exec", "backend", "--", "npm", "test"]);
        match args.command {
            Command::Exec {
                environment,
                command,
            } => {
                assert_eq!(environment, "backend");
                assert_eq!(command, vec!["npm", "test"]);
            }
            _ => panic!("Expected Command::Exec"),
        }
    }

    #[test]
    fn save_parses_humane_snapshot_name() {
        let args = Args::parse_from(["vm", "save", "backend", "as", "stable"]);
        match args.command {
            Command::Save { words, .. } => assert_eq!(words, vec!["backend", "as", "stable"]),
            _ => panic!("Expected Command::Save"),
        }
    }

    #[test]
    fn system_update_parses() {
        let args = Args::parse_from(["vm", "system", "update", "--force"]);
        match args.command {
            Command::System { command } => match command {
                SystemSubcommand::Update { force, .. } => assert!(force),
                _ => panic!("Expected SystemSubcommand::Update"),
            },
            _ => panic!("Expected Command::System"),
        }
    }

    #[test]
    fn plugin_install_parses() {
        let args = Args::parse_from(["vm", "plugin", "install", "/path/to/plugin"]);
        match args.command {
            Command::Plugin { command } => match command {
                PluginSubcommand::Install { source_path } => {
                    assert_eq!(source_path, "/path/to/plugin")
                }
                _ => panic!("Expected PluginSubcommand::Install"),
            },
            _ => panic!("Expected Command::Plugin"),
        }
    }

    #[test]
    fn db_remains_top_level_plugin_command() {
        let args = Args::parse_from(["vm", "db", "ls"]);
        match args.command {
            Command::Db { command } => match command {
                DbSubcommand::Ls => {}
                _ => panic!("Expected DbSubcommand::Ls"),
            },
            _ => panic!("Expected Command::Db"),
        }
    }
}
