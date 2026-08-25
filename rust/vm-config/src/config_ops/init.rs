// Standard library imports
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// External crate imports
use regex::Regex;
use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use tracing::{error, info, instrument, warn};
use vm_core::error::{Result, VmError};

// Internal crate imports
use vm_messages::messages::MESSAGES;

// Local module imports
use crate::config::VmConfig;
use crate::ports::{PortRange, PortRegistry};
use crate::yaml::core::CoreOperations;

mod preset;

use preset::build_config_from_preset;

// Compile regex patterns once at initialization for better performance
static INVALID_CHARS_RE: OnceLock<Regex> = OnceLock::new();
static CONSECUTIVE_HYPHENS_RE: OnceLock<Regex> = OnceLock::new();

fn get_invalid_chars_regex() -> &'static Regex {
    INVALID_CHARS_RE.get_or_init(|| {
        Regex::new(r"[^a-zA-Z0-9_-]")
            .expect("Hardcoded invalid characters regex pattern should always compile")
    })
}

fn get_consecutive_hyphens_regex() -> &'static Regex {
    CONSECUTIVE_HYPHENS_RE.get_or_init(|| {
        Regex::new(r"-+")
            .expect("Hardcoded consecutive hyphens regex pattern should always compile")
    })
}

#[instrument(skip(file_path, services, ports, preset))]
pub fn init_config_file(
    file_path: Option<PathBuf>,
    services: Option<String>,
    ports: Option<u16>,
    preset: Option<String>,
) -> Result<()> {
    // Determine target path
    let target_path = determine_target_path(file_path)?;

    // Check if vm.yaml already exists
    if target_path.exists() {
        print_already_exists_message(&target_path);
        return Err(VmError::Config(format!(
            "Configuration already exists: {}",
            target_path.display()
        )));
    }

    // Get current directory name for project name
    let current_dir = std::env::current_dir()?;
    let sanitized_name = sanitize_project_name(&current_dir)?;

    // Load and customize config
    let mut config = if let Some(preset_name) = preset {
        // Initialize with preset
        build_config_from_preset(&sanitized_name, &preset_name)?
    } else {
        // Use default initialization
        build_initial_config(&sanitized_name)?
    };

    // Allocate and register ports
    allocate_and_register_ports(&mut config, &sanitized_name, &current_dir)?;

    // Detect and configure services
    let services_to_configure = detect_services_from_project(services, &current_dir)?;
    apply_service_configurations(&mut config, services_to_configure)?;

    // Apply port configuration
    if let Some(port_start) = ports {
        if port_start < 1024 {
            return Err(VmError::Config(format!(
                "Invalid port number: {port_start} (must be >= 1024)"
            )));
        }
        config.ports.range = Some(vec![port_start, port_start + 9]);
    }

    // Allocate ports to enabled services
    config.ensure_service_ports();

    // Write config to file
    write_config_file(&target_path, &config)?;

    // Display success message
    print_success_message(&target_path, &sanitized_name, &config, ports);

    Ok(())
}

/// Determine the target file path for vm.yaml
fn determine_target_path(file_path: Option<PathBuf>) -> Result<PathBuf> {
    Ok(match file_path {
        Some(path) => {
            if path.is_dir() {
                path.join("vm.yaml")
            } else {
                path
            }
        }
        None => std::env::current_dir()?.join("vm.yaml"),
    })
}

/// Print message when vm.yaml already exists
fn print_already_exists_message(target_path: &Path) {
    info!("{}", MESSAGES.service.init_welcome);
    info!("");
    info!("{}", MESSAGES.service.init_already_exists);
    info!("   📁 {}", target_path.display());
    info!("");
    info!("{}", MESSAGES.service.init_options_hint);
    info!("   rm vm.yaml && vm run linux              # Start fresh");
    info!("   vm --config other.yaml run linux         # Create elsewhere");
    info!("   vm run linux                             # Use existing config");
}

/// Sanitize directory name for use as project name
fn sanitize_project_name(current_dir: &std::path::Path) -> Result<String> {
    let dir_name = current_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("vm-project");

    // Replace dots, spaces, and other invalid characters with hyphens
    let sanitized_name = get_invalid_chars_regex().replace_all(dir_name, "-");
    let sanitized_name = get_consecutive_hyphens_regex().replace_all(&sanitized_name, "-");
    let sanitized_name = sanitized_name.trim_matches('-');

    // If the sanitized name is different, inform the user
    if sanitized_name != dir_name {
        info!(
            "📝 Note: Directory name '{}' contains invalid characters for project names.",
            dir_name
        );
        info!("   Using sanitized name: '{}'", sanitized_name);
        info!("");
    }

    Ok(sanitized_name.to_string())
}

/// Build initial config from embedded defaults
fn build_initial_config(sanitized_name: &str) -> Result<VmConfig> {
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../../configs/defaults.yaml");
    let mut config: VmConfig = crate::yaml::CoreOperations::parse_yaml_with_diagnostics(
        EMBEDDED_DEFAULTS,
        "embedded defaults",
    )?;

    // Customize config for this directory
    if let Some(ref mut project) = config.project {
        project.name = Some(sanitized_name.to_string());
        project.hostname = Some(format!("dev.{sanitized_name}.local"));
    }

    if let Some(ref mut terminal) = config.terminal {
        terminal.username = Some(format!("{sanitized_name}-dev"));
    }

    // Add platform-aware swap defaults if not already set
    if let Some(ref mut vm) = config.vm {
        if vm.swap.is_none() || vm.swappiness.is_none() {
            let host_os = vm_platform::platform::detect_host_os();

            match host_os.as_str() {
                "macos" => {
                    if vm.swap.is_none() {
                        vm.swap = Some(crate::config::SwapLimit::Limited(1024));
                        // 1 GB for macOS
                    }
                    if vm.swappiness.is_none() {
                        vm.swappiness = Some(30); // Lower swappiness for macOS
                    }
                }
                "windows" => {
                    if vm.swap.is_none() {
                        vm.swap = Some(crate::config::SwapLimit::Limited(512)); // 512 MB for Windows
                    }
                    if vm.swappiness.is_none() {
                        vm.swappiness = Some(0); // Disabled on Windows
                    }
                }
                _ => {
                    // Linux and other Unix-like systems
                    if vm.swap.is_none() {
                        vm.swap = Some(crate::config::SwapLimit::Limited(2048));
                        // 2 GB default
                    }
                    if vm.swappiness.is_none() {
                        vm.swappiness = Some(60); // Standard Linux default
                    }
                }
            }
        }
    }

    Ok(config)
}

/// Allocate and register ports for the project
fn allocate_and_register_ports(
    config: &mut VmConfig,
    sanitized_name: &str,
    current_dir: &std::path::Path,
) -> Result<()> {
    if let Ok(registry) = PortRegistry::load() {
        // Check if this project already has ports registered
        let (range_str, is_new_project) =
            if let Some(existing_entry) = registry.get_entry(sanitized_name) {
                // Project already has ports - reuse them
                info!(
                    "♻️  Reusing existing port range {} for project '{}'",
                    existing_entry.range, sanitized_name
                );
                (Some(existing_entry.range.clone()), false)
            } else {
                // New project - suggest next available range
                (registry.suggest_next_range(10, 3000), true)
            };

        if let Some(range_str) = range_str {
            if let Ok(range) = PortRange::parse(&range_str) {
                config.ports.range = Some(vec![range.start, range.end]);

                // Register if this is a new project
                if is_new_project {
                    let mut registry = PortRegistry::load().unwrap_or_default();
                    let _ = registry
                        .register(sanitized_name, &range, &current_dir.to_string_lossy())
                        .map_err(|e| warn!("Failed to register port range: {}", e));
                }
            }
        } else {
            warn!("Could not find available port range");
        }
    } else {
        warn!("Failed to load port registry");
    }

    Ok(())
}

/// Detect services from project or use provided list
fn detect_services_from_project(
    services: Option<String>,
    current_dir: &std::path::Path,
) -> Result<Vec<String>> {
    match services {
        Some(ref services_str) => {
            // Manual service specification
            Ok(services_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect())
        }
        None => {
            // Smart detection
            detect_and_recommend_services(current_dir)
        }
    }
}

/// Apply service configurations to the config
fn apply_service_configurations(
    config: &mut VmConfig,
    services_to_configure: Vec<String>,
) -> Result<()> {
    for service in services_to_configure {
        let default_config = embedded_service_config(&service).ok_or_else(|| {
            error!("Unknown service: {}", service);
            error!("Available built-in services: postgresql, redis, mongodb, docker");
            VmError::Config("Service configuration not found".to_string())
        })?;
        let service_config: VmConfig = crate::yaml::CoreOperations::parse_yaml_with_diagnostics(
            default_config,
            &format!("embedded service config for {service}"),
        )?;

        // Extract only the specific service we want to enable from the service config
        if let Some(specific_service_config) = service_config.services.get(&service) {
            // Enable the specific service with its configuration
            let mut enabled_service = specific_service_config.clone();
            enabled_service.enabled = true;
            config.services.insert(service, enabled_service);
        }
    }

    Ok(())
}

fn embedded_service_config(service: &str) -> Option<&'static str> {
    match service {
        "postgresql" => Some(include_str!("../../resources/services/postgresql.yaml")),
        "redis" => Some(include_str!("../../resources/services/redis.yaml")),
        "mongodb" => Some(include_str!("../../resources/services/mongodb.yaml")),
        "docker" => Some(include_str!("../../resources/services/docker.yaml")),
        _ => None,
    }
}

/// Write config to YAML file
fn write_config_file(target_path: &PathBuf, config: &VmConfig) -> Result<()> {
    let config_yaml = serde_yaml::to_string(&config).map_err(|e| {
        VmError::Serialization(format!("Failed to serialize configuration to YAML: {e}"))
    })?;
    let config_value: Value =
        crate::yaml::CoreOperations::parse_yaml_with_diagnostics(&config_yaml, "generated config")?;

    CoreOperations::write_yaml_file(target_path, &config_value).map_err(|e| {
        VmError::Filesystem(format!(
            "Failed to write vm.yaml to {}: {}",
            target_path.display(),
            e
        ))
    })
}

/// Print success message with config details
fn print_success_message(
    target_path: &Path,
    sanitized_name: &str,
    config: &VmConfig,
    ports: Option<u16>,
) {
    // Get the port range for display
    let port_display = if let Some(range) = &config.ports.range {
        format!("{}-{}", range[0], range[1])
    } else if let Some(port_start) = ports {
        format!("{}-{}", port_start, port_start + 9)
    } else {
        "auto".to_string()
    };

    // Clean success output
    info!("{}", MESSAGES.service.init_welcome);
    info!("");
    info!("✓ Initializing project: {}", sanitized_name);
    info!("✓ Port range allocated: {}", port_display);

    // Display services with their assigned ports
    if !config.services.is_empty() {
        let enabled_services: Vec<_> = config.services.iter().filter(|(_, s)| s.enabled).collect();

        if !enabled_services.is_empty() {
            info!("✓ Services configured:");
            for (name, service) in enabled_services {
                if let Some(port) = service.port {
                    info!("    • {} (port {})", name, port);
                } else {
                    info!("    • {}", name);
                }
            }
        }
    }

    info!("✓ Configuration created: vm.yaml");
    info!("");
    info!("{}", MESSAGES.service.init_success);
    info!("{}", MESSAGES.service.init_next_steps);
    info!("   vm run linux # Launch your development environment");
    info!("   vm --help    # View all available commands");
    info!("");
    info!("📁 {}", target_path.display());
}

/// Detect project technologies and recommend services
fn detect_and_recommend_services(project_dir: &std::path::Path) -> Result<Vec<String>> {
    use crate::detector::get_detected_technologies;

    let detected = get_detected_technologies(project_dir);

    if !detected.is_empty() {
        let services = get_recommended_services(&detected);

        // Show what was detected
        let detected_list: Vec<String> = detected.iter().cloned().collect();
        info!("🔍 Detected: {}", detected_list.join(", "));
        if !services.is_empty() {
            info!("✓ Services: {}", services.join(", "));
        }

        Ok(services)
    } else {
        // No detection, no services
        Ok(vec![])
    }
}

/// Map detected technologies to recommended services
fn get_recommended_services(detected_types: &std::collections::HashSet<String>) -> Vec<String> {
    let mut services = Vec::new();

    let includes_any = |technologies: &[&str]| {
        technologies
            .iter()
            .any(|technology| detected_types.contains(*technology))
    };

    if includes_any(&[
        "nodejs", "react", "vue", "next", "angular", "python", "django", "flask", "rails", "ruby",
    ]) {
        services.push("postgresql".to_string());
    }
    if includes_any(&["python", "django", "flask", "rails", "ruby"]) {
        services.push("redis".to_string());
    }
    if detected_types.contains("docker") {
        services.push("docker".to_string());
    }

    services
}
