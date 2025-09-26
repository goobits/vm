//! Command-line interface module for the package server
//!
//! This module contains all CLI argument parsing, command definitions,
//! and command execution logic.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

use crate::{client_ops, docker_manager, wrapper};

/// Command-line interface for the package server
#[derive(Parser)]
#[command(name = "pkg-server")]
#[command(about = "Local package server for npm, cargo, and pip")]
#[command(
    after_help = "Server:\n  start     Start package server\n  stop      Stop background server\n  status    Show server status\n\nConfig:\n  use       Output shell functions (eval in .bashrc)\n  exec      Run single command via server\n\nPackages:\n  add       Publish package\n  remove    Remove package\n  list      List packages"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands for the package server
#[derive(Subcommand)]
pub enum Commands {
    // Server Management
    /// Start server (use --docker for ultra-simple Docker setup with team sharing)
    Start {
        /// Host to bind the server to
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
        /// Port to run the server on
        #[arg(short, long)]
        port: Option<u16>,
        /// Data directory for package storage
        #[arg(long, default_value = "./data")]
        data: PathBuf,
        /// Use ultra-simple Docker setup with auto-detection and team sharing
        #[arg(long, short = 'd')]
        docker: bool,
        /// Don't configure local package managers (local mode only)
        #[arg(long)]
        no_config: bool,
        /// Run server in foreground (local mode only)
        #[arg(long, short = 'f')]
        foreground: bool,
    },
    /// Stop the background server
    Stop,
    /// Show server status and package counts
    Status {
        /// Package server URL
        #[arg(long, default_value = "http://localhost:3080")]
        server: String,
    },

    // Package Operations
    /// Publish package from current directory
    Add {
        /// Package server URL
        #[arg(long, default_value = "http://localhost:3080")]
        server: String,
        /// Specify package type(s) to publish (python,npm,cargo)
        #[arg(long, short = 't')]
        r#type: Option<String>,
    },
    /// Remove package from server
    Remove {
        /// Package server URL
        #[arg(long, default_value = "http://localhost:3080")]
        server: String,
        /// Skip confirmation prompts and remove without interaction
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// List all packages on server
    List {
        /// Package server URL
        #[arg(long, default_value = "http://localhost:3080")]
        server: String,
    },

    // Configuration Commands
    /// Manage server configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Output shell functions for transparent server usage
    Use {
        /// Shell type (bash, zsh, fish, pwsh)
        #[arg(long)]
        shell: Option<String>,
        /// Package server port
        #[arg(long, default_value = "3080")]
        port: u16,
        /// Cache TTL in seconds
        #[arg(long, default_value = "5")]
        ttl: u64,
    },
    /// Execute command with local server (one-off)
    Exec {
        /// Command to execute
        command: String,
        /// Arguments to pass
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Configuration subcommands
#[derive(Subcommand, Clone)]
pub enum ConfigAction {
    /// Show all configuration values
    Show,
    /// Get a specific configuration value
    Get {
        /// Configuration key (e.g., port, host, data_dir)
        key: String,
    },
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// New value
        value: String,
    },
    /// Reset configuration to defaults
    Reset {
        /// Skip confirmation prompt
        #[arg(long, short = 'f')]
        force: bool,
    },
    /// Show configuration file path
    Path,
}

/// Execute the CLI command
pub async fn run() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "goobits_pkg_server=info,tower_http=debug".into()),
        )
        .init();

    let cli = Cli::parse();
    handle_command(cli.command).await
}

/// Handle individual commands
async fn handle_command(command: Commands) -> Result<()> {
    match command {
        Commands::Start {
            host,
            port,
            data,
            docker,
            no_config,
            foreground,
        } => handle_start(host, port, data, docker, no_config, foreground).await,

        Commands::Stop => handle_stop().await,

        Commands::Add { server, r#type } => handle_add(server, r#type).await,

        Commands::Remove { server, force } => handle_remove(server, force).await,

        Commands::List { server } => handle_list(server).await,

        Commands::Config { action } => handle_config(action).await,

        Commands::Use { shell, port, ttl } => handle_use(shell, port, ttl).await,

        Commands::Exec { command, args } => handle_exec(command, args).await,

        Commands::Status { server } => handle_status(server).await,
    }
}

/// Handle the start command
async fn handle_start(
    host: String,
    port: Option<u16>,
    data: PathBuf,
    docker: bool,
    no_config: bool,
    foreground: bool,
) -> Result<()> {
    if docker {
        // Ultra-simple Docker deployment
        tokio::task::block_in_place(docker_manager::deploy_quick)?;
        return Ok(());
    }

    // Load user configuration for defaults
    let user_config = vm_package_server::user_config::UserConfig::load().unwrap_or_default();

    // Use provided values or fall back to user config, then system defaults
    let actual_host = if host != "0.0.0.0" {
        host
    } else {
        user_config.server.host.clone()
    };

    let actual_port = port.unwrap_or(user_config.server.port);

    let actual_data = if data != PathBuf::from("./data") {
        data
    } else {
        user_config.server.data_dir.clone()
    };

    // Start the server
    info!(version = env!("CARGO_PKG_VERSION"), "Starting pkg-server");

    let result = if foreground {
        // Run in foreground mode
        crate::run_server(actual_host, actual_port, actual_data).await
    } else {
        // Run in background mode
        crate::run_server_background(actual_host, actual_port, actual_data).await
    };

    // Show configuration instructions after successful start
    if result.is_ok() && !no_config {
        // Check if auto_configure is enabled
        if user_config.client.auto_configure {
            println!("\n🔧 Auto-configuring package managers...");
            // Generate and display shell functions for current shell
            let shell = std::env::var("SHELL")
                .ok()
                .and_then(|s| s.split('/').next_back().map(String::from));

            if let Err(e) = wrapper::generate_shell_functions(shell, actual_port, 5) {
                eprintln!("⚠️  Could not auto-configure: {}", e);
            } else {
                println!("✅ Package managers configured for this session");
                println!("\n💡 To make this permanent, add to your shell config:");
                println!("   eval \"$(pkg-server use)\"");
            }
        } else {
            println!("\n📦 To use this server for package installations:");
            println!("   eval \"$(pkg-server use)\"");
            println!("\n🎯 For one-off usage:");
            println!("   pkg-server exec npm install express");
            println!("\n🔄 To stop using the local server:");
            println!("   Restart your shell or unset the functions");
        }
    }

    result
}

/// Handle the stop command
async fn handle_stop() -> Result<()> {
    // Try to stop the background server if it's running
    let data_dir = PathBuf::from("./data");
    let pid_file_path = data_dir.join(".pkg-server.pid");

    if pid_file_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                // Try to terminate the process
                unsafe {
                    if libc::kill(pid, libc::SIGTERM) == 0 {
                        println!("✅ Background server stopped (PID: {})", pid);
                        // Clean up the PID file
                        let _ = std::fs::remove_file(&pid_file_path);
                    } else {
                        println!(
                            "⚠️  Could not stop server (PID: {}). It may have already stopped.",
                            pid
                        );
                        // Clean up stale PID file
                        let _ = std::fs::remove_file(&pid_file_path);
                    }
                }
            } else {
                println!("⚠️  Invalid PID in .pkg-server.pid file");
                let _ = std::fs::remove_file(&pid_file_path);
            }
        } else {
            println!("⚠️  Could not read PID file");
        }
    } else {
        println!("ℹ️  No background server is currently running");
    }

    Ok(())
}

/// Handle the add command
async fn handle_add(server: String, package_type: Option<String>) -> Result<()> {
    // Check if server is running, and auto-start if configured
    let user_config = vm_package_server::user_config::UserConfig::load().unwrap_or_default();

    if user_config.server.auto_start {
        // Check if server is running
        let client = vm_package_server::api::PackageServerClient::new(&server);
        if !client.is_server_running() {
            println!("🚀 Auto-starting server (configured in settings)...");
            // Start server in background with configured settings
            let _ = crate::run_server_background(
                user_config.server.host.clone(),
                user_config.server.port,
                user_config.server.data_dir.clone(),
            )
            .await;
            // Give server time to start
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }

    tokio::task::block_in_place(|| client_ops::add_package(&server, package_type.as_deref()))?;
    Ok(())
}

/// Handle the remove command
async fn handle_remove(server: String, force: bool) -> Result<()> {
    tokio::task::block_in_place(|| client_ops::remove_package(&server, force))?;
    Ok(())
}

/// Handle the list command
async fn handle_list(server: String) -> Result<()> {
    tokio::task::block_in_place(|| client_ops::list_packages(&server))?;
    Ok(())
}

/// Handle the use command
async fn handle_use(shell: Option<String>, port: u16, ttl: u64) -> Result<()> {
    // Use default_shell from config if not specified
    let user_config = vm_package_server::user_config::UserConfig::load().unwrap_or_default();
    let actual_shell = shell.or(user_config.client.default_shell);

    wrapper::generate_shell_functions(actual_shell, port, ttl)?;
    Ok(())
}

/// Handle the exec command
async fn handle_exec(command: String, args: Vec<String>) -> Result<()> {
    let exit_code = wrapper::exec_with_wrapper(&command, &args).await?;
    std::process::exit(exit_code);
}

/// Handle the status command
async fn handle_status(server: String) -> Result<()> {
    // Check if background server is running locally
    let data_dir = PathBuf::from("./data");
    let pid_file_path = data_dir.join(".pkg-server.pid");

    if pid_file_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&pid_file_path) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                // Check if process is actually running
                let running = unsafe { libc::kill(pid, 0) == 0 };

                if running {
                    println!("✅ Background server is running (PID: {})", pid);
                } else {
                    println!("⚠️  PID file exists but process is not running. Cleaning up...");
                    let _ = std::fs::remove_file(&pid_file_path);
                }
            }
        }
    } else {
        println!("ℹ️  No background server is currently running");
    }

    // Now check the server status via HTTP
    tokio::task::block_in_place(|| client_ops::show_status(&server))?;
    Ok(())
}

/// Handle configuration management
async fn handle_config(action: Option<ConfigAction>) -> Result<()> {
    use vm_package_server::user_config::UserConfig;

    let action = action.unwrap_or(ConfigAction::Show);

    match action {
        ConfigAction::Show => {
            let config = UserConfig::load()?;
            println!("{}", config.display());
        }
        ConfigAction::Get { key } => {
            let config = UserConfig::load()?;
            match config.get(&key) {
                Ok(value) => println!("{}", value),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    eprintln!(
                        "\nAvailable keys: {}",
                        UserConfig::available_keys().join(", ")
                    );
                    std::process::exit(1);
                }
            }
        }
        ConfigAction::Set { key, value } => {
            let mut config = UserConfig::load()?;
            match config.set(&key, &value) {
                Ok(()) => {
                    config.save()?;
                    println!("✅ Set {} = {}", key, value);
                    println!("\n💡 Changes will take effect on next server start");
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    eprintln!(
                        "\nAvailable keys: {}",
                        UserConfig::available_keys().join(", ")
                    );
                    std::process::exit(1);
                }
            }
        }
        ConfigAction::Reset { force } => {
            if !force {
                println!("This will reset all configuration to defaults.");
                print!("Continue? [y/N]: ");
                use std::io::{self, Write};
                io::stdout().flush()?;

                let mut response = String::new();
                io::stdin().read_line(&mut response)?;

                if !response.trim().eq_ignore_ascii_case("y") {
                    println!("Reset cancelled");
                    return Ok(());
                }
            }

            let config = UserConfig::default();
            config.save()?;
            println!("✅ Configuration reset to defaults");
        }
        ConfigAction::Path => match UserConfig::config_path() {
            Ok(path) => println!("{}", path.display()),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
    }

    Ok(())
}
