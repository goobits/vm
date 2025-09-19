use crate::config::VmConfig;
use crate::ports::{PortRange, PortRegistry};
use crate::yaml::core::CoreOperations;
use anyhow::{Context, Result};
use regex::Regex;
use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use std::path::PathBuf;
use std::sync::OnceLock;
use vm_common::{vm_error, vm_success, vm_warning};

// Compile regex patterns once at initialization for better performance
static INVALID_CHARS_RE: OnceLock<Regex> = OnceLock::new();
static CONSECUTIVE_HYPHENS_RE: OnceLock<Regex> = OnceLock::new();

fn get_invalid_chars_regex() -> &'static Regex {
    INVALID_CHARS_RE.get_or_init(|| {
        Regex::new(r"[^a-zA-Z0-9_-]").unwrap_or_else(|_| {
            // Fallback to a safe pattern if the main one fails
            Regex::new(r"[^\w-]").unwrap_or_else(|_| Regex::new(r"a^").unwrap())
        })
    })
}

fn get_consecutive_hyphens_regex() -> &'static Regex {
    CONSECUTIVE_HYPHENS_RE.get_or_init(|| {
        Regex::new(r"-+").unwrap_or_else(|_| {
            // Fallback to a safe pattern if the main one fails
            Regex::new(r"--+").unwrap_or_else(|_| Regex::new(r"a^").unwrap())
        })
    })
}

pub fn execute(
    file_path: Option<PathBuf>,
    services: Option<String>,
    ports: Option<u16>,
) -> Result<()> {
    // Determine target path
    let target_path = match file_path {
        Some(path) => {
            if path.is_dir() {
                path.join("vm.yaml")
            } else {
                path
            }
        }
        None => std::env::current_dir()?.join("vm.yaml"),
    };

    // Check if vm.yaml already exists
    if target_path.exists() {
        vm_error!(
            "vm.yaml already exists at {}\nUse --file to specify a different location or remove the existing file.",
            target_path.display()
        );
        return Err(anyhow::anyhow!("vm.yaml already exists"));
    }

    // Get current directory name for project name
    let current_dir = std::env::current_dir()?;
    let dir_name = current_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("vm-project");

    // Sanitize directory name for use as project name
    // Replace dots, spaces, and other invalid characters with hyphens
    // Then remove any consecutive hyphens and trim leading/trailing hyphens
    let sanitized_name = get_invalid_chars_regex().replace_all(dir_name, "-");
    let sanitized_name = get_consecutive_hyphens_regex().replace_all(&sanitized_name, "-");
    let sanitized_name = sanitized_name.trim_matches('-');

    // If the sanitized name is different, inform the user
    if sanitized_name != dir_name {
        println!(
            "📝 Note: Directory name '{}' contains invalid characters for project names.",
            dir_name
        );
        println!("   Using sanitized name: '{}'", sanitized_name);
        println!();
    }

    // Load embedded defaults
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../../../defaults.yaml");
    let mut config: VmConfig =
        serde_yaml::from_str(EMBEDDED_DEFAULTS).context("Failed to parse embedded defaults")?;

    // Customize config for this directory
    if let Some(ref mut project) = config.project {
        project.name = Some(sanitized_name.to_string());
        project.hostname = Some(format!("dev.{}.local", sanitized_name));
    }

    if let Some(ref mut terminal) = config.terminal {
        terminal.username = Some(format!("{}-dev", sanitized_name));
    }

    // Use vm-ports library to suggest and register an available port range
    if let Ok(registry) = PortRegistry::load() {
        if let Some(range_str) = registry.suggest_next_range(10, 3000) {
            config.port_range = Some(range_str.clone());
            println!("📡 Allocated port range: {}", range_str);

            // Register this range
            if let Ok(range) = PortRange::parse(&range_str) {
                let mut registry = PortRegistry::load().unwrap_or_else(|_| {
                    vm_warning!("Failed to load port registry, using default");
                    PortRegistry::default()
                });
                if let Err(e) =
                    registry.register(sanitized_name, &range, &current_dir.to_string_lossy())
                {
                    vm_warning!("Failed to register port range: {}", e);
                }
            }
        } else {
            vm_warning!("Could not find available port range");
        }
    } else {
        vm_warning!("Failed to load port registry");
    }

    // Apply service configurations
    if let Some(ref services_str) = services {
        let service_list: Vec<&str> = services_str.split(',').map(|s| s.trim()).collect();

        for service in service_list {
            // Load service config
            let service_path =
                crate::paths::resolve_tool_path(format!("configs/services/{}.yaml", service));
            if !service_path.exists() {
                vm_error!("Unknown service: {}", service);
                vm_error!("Available services: postgresql, redis, mongodb, docker");
                return Err(anyhow::anyhow!("Service configuration not found"));
            }

            let service_config = VmConfig::from_file(&service_path)
                .with_context(|| format!("Failed to load service config: {}", service))?;

            // Extract only the specific service we want to enable from the service config
            if let Some(specific_service_config) = service_config.services.get(service) {
                // Enable the specific service with its configuration
                let mut enabled_service = specific_service_config.clone();
                enabled_service.enabled = true;
                config.services.insert(service.to_string(), enabled_service);
            }
        }
    }

    // Apply port configuration
    if let Some(port_start) = ports {
        if port_start < 1024 {
            return Err(anyhow::anyhow!(
                "Invalid port number: {} (must be >= 1024)",
                port_start
            ));
        }

        // Allocate sequential ports - use &str literals to avoid String allocation
        config.ports.insert("web".to_string(), port_start);
        config.ports.insert("api".to_string(), port_start + 1);
        config
            .ports
            .insert("postgresql".to_string(), port_start + 5);
        config.ports.insert("redis".to_string(), port_start + 6);
        config.ports.insert("mongodb".to_string(), port_start + 7);
    }

    // Convert config to Value and write with consistent formatting
    let config_yaml =
        serde_yaml::to_string(&config).context("Failed to serialize configuration to YAML")?;
    let config_value: Value =
        serde_yaml::from_str(&config_yaml).context("Failed to convert config to YAML Value")?;

    // Write using the centralized function for consistent formatting
    CoreOperations::write_yaml_file(&target_path, &config_value).context(format!(
        "Failed to write vm.yaml to {}",
        target_path.display()
    ))?;

    vm_success!("Created vm.yaml for project: {}", sanitized_name);
    println!("📍 Configuration file: {}", target_path.display());
    if let Some(ref services_str) = services {
        println!("🔧 Services: {}", services_str);
    }
    if let Some(port_start) = ports {
        println!("🔌 Port range: {}-{}", port_start, port_start + 9);
    }
    println!();
    println!("Next steps:");
    println!("  1. Review and customize vm.yaml as needed");
    println!("  2. Run \"vm create\" to start your development environment");

    Ok(())
}
