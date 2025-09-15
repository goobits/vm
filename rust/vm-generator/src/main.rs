use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::fs;
use vm_config::config::VmConfig;
use vm_config::merge::ConfigMerger;
use vm_config::resolve_tool_path;

#[derive(Debug, Parser)]
#[command(name = "vm-generator")]
#[command(about = "Configuration generator for VM Tool")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Generate configuration by composing services
    Generate {
        /// Comma-separated list of services (postgresql,redis,docker)
        #[arg(long)]
        services: Option<String>,

        /// Starting port for allocation (allocates 10 sequential ports)
        #[arg(long)]
        ports: Option<u16>,

        /// Project name (sets project.name, hostname, username)
        #[arg(long)]
        name: Option<String>,

        /// Output file (default: vm.yaml)
        output: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Generate { services, ports, name, output } => {
            handle_generate_command(services, ports, name, output)
        }
    }
}

// Generate command implementation
fn handle_generate_command(
    services: Option<String>,
    ports: Option<u16>,
    name: Option<String>,
    output: Option<PathBuf>,
) -> Result<()> {
    println!("⚙️ Generating configuration...");

    // Load base config from defaults
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../defaults.yaml");
    let mut config: VmConfig = serde_yaml::from_str(EMBEDDED_DEFAULTS)
        .context("Failed to parse embedded defaults")?;

    // Parse and merge services
    if let Some(ref services_str) = services {
        let service_list: Vec<String> = services_str
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        for service in service_list {
            // Load service config
            let service_path = resolve_tool_path(format!("configs/services/{}.yaml", service));
            if !service_path.exists() {
                eprintln!("❌ Unknown service: {}", service);
                eprintln!("💡 Available services: postgresql, redis, mongodb, docker");
                return Err(anyhow::anyhow!("Service configuration not found"));
            }

            let service_config = VmConfig::from_file(&service_path)
                .with_context(|| format!("Failed to load service config: {}", service))?;

            // Merge service config into base
            config = ConfigMerger::new(config)
                .merge(service_config)?;
        }
    }

    // Apply port configuration
    if let Some(port_start) = ports {
        if port_start < 1024 || port_start > 65535 {
            return Err(anyhow::anyhow!("Invalid port number: {} (must be between 1024-65535)", port_start));
        }

        // Allocate sequential ports
        config.ports.insert("web".to_string(), port_start);
        config.ports.insert("api".to_string(), port_start + 1);
        config.ports.insert("postgresql".to_string(), port_start + 5);
        config.ports.insert("redis".to_string(), port_start + 6);
        config.ports.insert("mongodb".to_string(), port_start + 7);
    }

    // Apply project name
    if let Some(ref project_name) = name {
        if let Some(ref mut project) = config.project {
            project.name = Some(project_name.clone());
            project.hostname = Some(format!("dev.{}.local", project_name));
        }
        if let Some(ref mut terminal) = config.terminal {
            terminal.username = Some(format!("{}-dev", project_name));
        }
    }

    // Write to output file
    let output_path = output.unwrap_or_else(|| PathBuf::from("vm.yaml"));
    let yaml = serde_yaml::to_string(&config)?;
    fs::write(&output_path, yaml).context("Failed to write configuration file")?;

    println!("✅ Generated {}", output_path.display());
    if let Some(ref services_str) = services {
        println!("   Services: {}", services_str);
    }
    if let Some(port_start) = ports {
        println!("   Port range: {}-{}", port_start, port_start + 9);
    }
    if let Some(ref project_name) = name {
        println!("   Project name: {}", project_name);
    }

    Ok(())
}