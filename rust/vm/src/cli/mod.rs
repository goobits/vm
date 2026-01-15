// CLI argument parsing and definitions

// Standard library imports
use std::path::PathBuf;

// External crate imports
use clap::{Parser, Subcommand};

#[derive(Debug, Clone, Parser)]
#[command(name = "vm")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(author = "Goobits VM Contributors")]
#[command(about = "Smart development environments for modern projects")]
#[command(before_help = format!(" \nvm v{}", env!("CARGO_PKG_VERSION")))]
#[command(after_help = " \n")]
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
        /// Value(s) to set (multiple values for arrays: networking.networks val1 val2)
        #[arg(required = true, num_args = 1..)]
        values: Vec<String>,
        /// Apply to global configuration (~/.config/vm/global.yaml)
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
        /// Preset names (comma-separated for multiple, e.g., "nodejs,docker")
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
    List,
    /// Set the default profile for this project
    Set {
        /// Profile name to use when no --profile is provided
        name: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TempSubcommand {
    /// Create a temporary environment
    Create {
        /// Directories to mount (e.g., ./src,./config:ro)
        mounts: Vec<String>,

        /// Automatically destroy VM on exit
        #[arg(long)]
        auto_destroy: bool,
    },
    /// Connect to your temp environment
    Ssh,
    /// Check temp environment status
    Status,
    /// Delete your temp environment
    Destroy,
    /// Add a folder to your temp environment
    Mount {
        /// Path to mount (e.g., ./src or ./config:ro)
        path: String,
        /// Skip confirmation prompts
        #[arg(long)]
        yes: bool,
    },
    /// Remove a folder from your temp environment
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
    /// See mounted folders
    Mounts,
    /// See all temp environments
    List,
    /// Stop your temp environment
    Stop,
    /// Start your temp environment
    Start,
    /// Restart your temp environment
    Restart,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RegistrySubcommand {
    /// Check registry server status
    Status {
        /// Start server automatically without prompting
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Publish a package to the registry
    Add {
        /// Specify package type(s) to publish (python,npm,cargo)
        #[arg(long, short = 't')]
        r#type: Option<String>,
        /// Start server automatically without prompting
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Remove a package from the registry
    Remove {
        /// Skip confirmation prompts
        #[arg(long, short = 'f')]
        force: bool,
        /// Start server automatically without prompting
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// List packages in the registry
    List {
        /// Start server automatically without prompting
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
        /// Shell type (bash, zsh, fish)
        #[arg(long)]
        shell: Option<String>,
        /// Registry server port
        #[arg(long, default_value = "3080")]
        port: u16,
    },
    /// Start registry server (internal use - for background process)
    #[command(hide = true)]
    Serve {
        /// Host to bind to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        /// Port to bind to
        #[arg(long, default_value = "3080")]
        port: u16,
        /// Data directory for package storage
        #[arg(long)]
        data: std::path::PathBuf,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum RegistryConfigAction {
    /// View all settings
    Show,
    /// Get a specific setting
    Get {
        /// Configuration key
        key: String,
    },
    /// Change a setting
    Set {
        /// Configuration key
        key: String,
        /// New value
        value: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum TunnelSubcommand {
    /// Create a tunnel (e.g., vm tunnel 8080:3000)
    #[command(name = "create", visible_alias = "forward")]
    Create {
        /// Port mapping (e.g., 8080:3000 maps localhost:8080 to container:3000)
        mapping: String,
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
    },
    /// List active tunnels
    List {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
    },
    /// Stop tunnel(s)
    Stop {
        /// Host port to stop (omit to stop all tunnels)
        port: Option<u16>,
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
        /// Stop all tunnels for this container
        #[arg(long)]
        all: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SecretsSubcommand {
    /// Check secrets proxy status
    Status,
    /// Store a secret
    Add {
        /// Secret name
        name: String,
        /// Secret value
        value: String,
        /// Secret scope (global, project:NAME, instance:NAME)
        #[arg(long)]
        scope: Option<String>,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
    },
    /// See all secrets
    List {
        /// Show secret values (masked)
        #[arg(long)]
        show_values: bool,
    },
    /// Delete a secret
    Remove {
        /// Secret name
        name: String,
        /// Skip confirmation prompt
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
        /// The name of the database to backup (omit if using --all)
        db_name: Option<String>,
        /// Optional backup name
        name: Option<String>,
        /// Backup all databases (excludes system databases)
        #[arg(long)]
        all: bool,
    },
    /// Restore a database from a backup
    Restore {
        /// Backup name to restore
        name: String,
        /// Target database name
        db_name: String,
    },
    /// List all databases and backups
    List,
    /// Export a database to a SQL file
    Export {
        /// Database name to export
        name: String,
        /// File path to export to
        file: PathBuf,
    },
    /// Import a database from a SQL file
    Import {
        /// File path to import from
        file: PathBuf,
        /// Target database name
        db_name: String,
    },
    /// Show disk usage per database
    Size,
    /// Drop and recreate a database
    Reset {
        /// Database name to reset
        name: String,
        /// Force reset without confirmation
        #[arg(long)]
        force: bool,
    },
    /// Show credentials for a database service
    Credentials {
        /// The name of the service (e.g., postgresql, redis, mongodb)
        service: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SnapshotSubcommand {
    /// Create a snapshot of the current VM state
    Create {
        /// Snapshot name
        name: String,
        /// Optional description
        #[arg(long)]
        description: Option<String>,
        /// Stop services before snapshotting for consistency
        #[arg(long)]
        quiesce: bool,
        /// Project name (auto-detected if omitted)
        #[arg(long)]
        project: Option<String>,
        /// Build snapshot directly from a Dockerfile
        #[arg(long, value_name = "PATH")]
        from_dockerfile: Option<PathBuf>,
        /// Build context directory for Dockerfile (defaults to current directory)
        #[arg(long, value_name = "PATH", default_value = ".")]
        build_context: Option<PathBuf>,
        /// Build arguments for Dockerfile (repeatable: --build-arg KEY=VALUE)
        #[arg(long, value_name = "KEY=VALUE")]
        build_arg: Vec<String>,
        /// Overwrite existing snapshot with same name
        #[arg(long)]
        force: bool,
    },
    /// List available snapshots
    List {
        /// Filter by project name
        #[arg(long)]
        project: Option<String>,
        /// Filter by snapshot type (base or project)
        #[arg(long, value_parser = ["base", "project"])]
        r#type: Option<String>,
    },
    /// Restore VM from a snapshot
    Restore {
        /// Snapshot name to restore
        name: String,
        /// Project name (auto-detected if omitted)
        #[arg(long)]
        project: Option<String>,
        /// Force restore without confirmation
        #[arg(long)]
        force: bool,
    },
    /// Delete a snapshot
    Delete {
        /// Snapshot name to delete
        name: String,
        /// Project name (auto-detected if omitted)
        #[arg(long)]
        project: Option<String>,
        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },
    /// Export a snapshot to a portable file
    Export {
        /// Snapshot name to export (use @name for global snapshots)
        name: String,
        /// Output file path (default: <name>.snapshot.tar.gz)
        #[arg(long, short = 'o')]
        output: Option<PathBuf>,
        /// Compression level (1-9, default: 6)
        #[arg(long, default_value = "6")]
        compress: u8,
        /// Project name (auto-detected if omitted, not needed for @global snapshots)
        #[arg(long)]
        project: Option<String>,
    },
    /// Import a snapshot from a portable file
    Import {
        /// Path to snapshot file (.snapshot.tar.gz)
        file: PathBuf,
        /// Override snapshot name (default: use name from file)
        #[arg(long)]
        name: Option<String>,
        /// Verify checksum before importing
        #[arg(long)]
        verify: bool,
        /// Overwrite existing snapshot with same name
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum PluginSubcommand {
    /// See installed plugins
    List,
    /// Get plugin details
    Info {
        /// Plugin name
        plugin_name: String,
    },
    /// Add a plugin
    Install {
        /// Path to plugin directory
        source_path: String,
    },
    /// Remove a plugin
    Remove {
        /// Plugin name to remove
        plugin_name: String,
    },
    /// Create a new plugin
    New {
        /// Plugin name
        plugin_name: String,
        /// Plugin type (preset or service)
        #[arg(long)]
        r#type: String,
    },
    /// Check plugin configuration
    Validate {
        /// Plugin name to validate
        plugin_name: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Zero to code in one command (init → create → start → ssh)
    #[command(about = "Get from zero to coding in one command")]
    Up {
        /// Command to execute (if not provided, opens interactive shell)
        #[arg(short = 'c', long)]
        command: Option<String>,
        /// Wait for services to be ready before continuing
        #[arg(long)]
        wait: bool,
    },
    /// Stop your environment
    Down {
        /// Container name or ID to stop (if not provided, stops current project VM gracefully)
        container: Option<String>,
    },
    /// Run health checks and diagnostics
    #[command(about = "Check system dependencies, configuration, and service health")]
    Doctor {
        /// Attempt to automatically fix issues
        #[arg(long)]
        fix: bool,
        /// Clean up unused resources
        #[arg(long)]
        clean: bool,
    },
    /// Update configuration settings
    Config {
        #[command(subcommand)]
        command: ConfigSubcommand,
    },
    /// Delete an environment
    Destroy {
        /// Container name, ID, or project name to destroy
        #[arg()]
        container: Option<String>,
        /// Force destruction without confirmation
        #[arg(long)]
        force: bool,
        /// Do not create a backup before destroying
        #[arg(long)]
        no_backup: bool,
        /// Destroy all instances across all providers
        #[arg(long)]
        all: bool,
        /// Destroy all instances from specific provider
        #[arg(long)]
        provider: Option<String>,
        /// Match pattern for instance names (e.g., "*-dev")
        #[arg(long)]
        pattern: Option<String>,
        /// Reuse existing service containers (postgres, redis, etc.) instead of creating new ones
        #[arg(long, default_value = "true")]
        preserve_services: bool,
    },

    /// Check environment status (defaults to listing all environments)
    Status {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
    },
    /// Manage port tunnels to your environment
    Tunnel {
        #[command(subcommand)]
        command: TunnelSubcommand,
    },
    /// Jump into your environment
    Ssh {
        /// Container name, ID, or project name to connect to
        #[arg()]
        container: Option<String>,
        /// Directory path to start shell in
        #[arg(long)]
        path: Option<PathBuf>,
        /// Command to execute (if not provided, opens interactive shell)
        #[arg(short = 'e', long = "command")]
        command: Option<String>,

        /// Force refresh mounts (disconnects other sessions)
        #[arg(long)]
        force_refresh: bool,

        /// Skip automatic mount refresh detection
        #[arg(long)]
        no_refresh: bool,
    },
    /// Run a command in your environment
    Exec {
        /// Container name, ID, or project name
        #[arg(long)]
        container: Option<String>,
        /// Command to execute inside VM
        #[arg(required = true, num_args = 1..)]
        command: Vec<String>,
    },
    /// View environment logs
    Logs {
        /// Container name, ID, or project name
        #[arg()]
        container: Option<String>,
        /// Follow log output (live stream)
        #[arg(short = 'f', long)]
        follow: bool,
        /// Number of lines to show from end of logs
        #[arg(short = 'n', long, default_value = "50")]
        tail: usize,
        /// Show logs for specific service (postgresql, redis, mongodb, mysql)
        #[arg(short = 's', long)]
        service: Option<String>,
    },
    /// Copy files to/from your environment
    Copy {
        /// Source path (local file or <container>:/path)
        source: String,
        /// Destination path (local file or <container>:/path)
        destination: String,
        /// Copy to/from all running containers
        #[arg(long)]
        all_vms: bool,
    },

    /// Work with temporary environments
    Temp {
        #[command(subcommand)]
        command: TempSubcommand,
    },

    /// Manage private package registry
    Registry {
        #[command(subcommand)]
        command: RegistrySubcommand,
    },

    /// Manage secrets and credentials
    Secrets {
        #[command(subcommand)]
        command: SecretsSubcommand,
    },

    /// Manage databases
    Db {
        #[command(subcommand)]
        command: DbSubcommand,
    },

    /// Manage VM snapshots
    Snapshot {
        #[command(subcommand)]
        command: SnapshotSubcommand,
    },

    /// Extend with plugins
    Plugin {
        #[command(subcommand)]
        command: PluginSubcommand,
    },

    /// Generate shell completion scripts
    Completion {
        /// Shell type (bash, zsh, fish, powershell)
        shell: String,
    },

    /// Update to the latest version
    Update {
        /// Specific version to install (e.g., v1.2.3)
        #[arg(long)]
        version: Option<String>,
        /// Force update even if already at latest version
        #[arg(long)]
        force: bool,
    },
    /// Remove from your system
    Uninstall {
        /// Keep configuration files
        #[arg(long)]
        keep_config: bool,
        /// Skip confirmation prompt
        #[arg(long, short = 'y')]
        yes: bool,
    },
    /// Get workspace directory
    #[command(hide = true)]
    GetSyncDirectory,
}

#[cfg(test)]
mod tests {
    use super::{
        Args, Command, PluginSubcommand, RegistrySubcommand, SecretsSubcommand, TempSubcommand,
    };
    use clap::Parser;

    #[test]
    fn test_up_command_parsing() {
        let args = Args::parse_from(["vm", "up", "-c", "echo hi", "--wait"]);
        match args.command {
            Command::Up { command, wait } => {
                assert_eq!(command, Some("echo hi".to_string()));
                assert!(wait);
            }
            _ => panic!("Expected Command::Up"),
        }
    }

    #[test]
    fn test_down_command_parsing() {
        let args = Args::parse_from(["vm", "down", "my-container"]);
        match args.command {
            Command::Down { container } => {
                assert_eq!(container, Some("my-container".to_string()));
            }
            _ => panic!("Expected Command::Down"),
        }
    }

    #[test]
    fn test_temp_create_command_parsing() {
        let args = Args::parse_from([
            "vm",
            "temp",
            "create",
            "--auto-destroy",
            "./src",
            "./config:ro",
        ]);
        match args.command {
            Command::Temp { command } => match command {
                TempSubcommand::Create {
                    mounts,
                    auto_destroy,
                } => {
                    assert!(auto_destroy);
                    assert_eq!(mounts, vec!["./src", "./config:ro"]);
                }
                _ => panic!("Expected TempSubcommand::Create"),
            },
            _ => panic!("Expected Command::Temp"),
        }
    }

    #[test]
    fn test_pkg_add_command_parsing() {
        let args = Args::parse_from(["vm", "registry", "add", "--type", "python", "-y"]);
        match args.command {
            Command::Registry { command } => match command {
                RegistrySubcommand::Add { r#type, yes } => {
                    assert_eq!(r#type, Some("python".to_string()));
                    assert!(yes);
                }
                _ => panic!("Expected RegistrySubcommand::Add"),
            },
            _ => panic!("Expected Command::Registry"),
        }
    }

    #[test]
    fn test_secrets_list_command_parsing() {
        let args = Args::parse_from(["vm", "secrets", "list", "--show-values"]);
        match args.command {
            Command::Secrets { command } => match command {
                SecretsSubcommand::List { show_values } => {
                    assert!(show_values);
                }
                _ => panic!("Expected SecretsSubcommand::List"),
            },
            _ => panic!("Expected Command::Secrets"),
        }
    }

    #[test]
    fn test_plugin_install_command_parsing() {
        let args = Args::parse_from(["vm", "plugin", "install", "/path/to/plugin"]);
        match args.command {
            Command::Plugin { command } => match command {
                PluginSubcommand::Install { source_path } => {
                    assert_eq!(source_path, "/path/to/plugin");
                }
                _ => panic!("Expected PluginSubcommand::Install"),
            },
            _ => panic!("Expected Command::Plugin"),
        }
    }

    #[test]
    fn test_exec_command_parsing() {
        let args = Args::parse_from([
            "vm",
            "exec",
            "--container",
            "my-vm",
            "--",
            "ls",
            "-la",
            "/root",
        ]);
        match args.command {
            Command::Exec { container, command } => {
                assert_eq!(container, Some("my-vm".to_string()));
                assert_eq!(command, vec!["ls", "-la", "/root"]);
            }
            _ => panic!("Expected Command::Exec"),
        }
    }

    #[test]
    fn test_global_flags_parsing() {
        let args = Args::parse_from(["vm", "--config", "/custom/config.yaml", "status"]);
        assert_eq!(
            args.config,
            Some(std::path::PathBuf::from("/custom/config.yaml"))
        );
        match args.command {
            Command::Status { .. } => { /* Correct command */ }
            _ => panic!("Expected Command::Status"),
        }
    }
}
