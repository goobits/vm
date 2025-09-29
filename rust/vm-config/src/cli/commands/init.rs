// Standard library imports
use std::path::PathBuf;
use std::sync::OnceLock;

// External crate imports
use regex::Regex;
use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use vm_core::error::{Result, VmError};

// Internal crate imports
use vm_core::{vm_error, vm_println, vm_warning};
use vm_messages::messages::MESSAGES;

// Local module imports
use crate::config::VmConfig;
use crate::ports::{PortRange, PortRegistry};
use crate::yaml::core::CoreOperations;

// Compile regex patterns once at initialization for better performance
static INVALID_CHARS_RE: OnceLock<Regex> = OnceLock::new();
static CONSECUTIVE_HYPHENS_RE: OnceLock<Regex> = OnceLock::new();

fn get_invalid_chars_regex() -> &'static Regex {
    INVALID_CHARS_RE.get_or_init(|| {
        Regex::new(r"[^a-zA-Z0-9_-]").unwrap_or_else(|_| {
            // Fallback to a safe pattern if the main one fails
            Regex::new(r"[^\w-]").unwrap_or_else(|_| {
                // Final fallback - use simple pattern that cannot fail
                Regex::new(r"").unwrap_or_else(|_| {
                    panic!("Critical: Even empty regex pattern is failing - regex engine corrupted")
                })
            })
        })
    })
}

fn get_consecutive_hyphens_regex() -> &'static Regex {
    CONSECUTIVE_HYPHENS_RE.get_or_init(|| {
        Regex::new(r"-+").unwrap_or_else(|_| {
            // Fallback to a safe pattern if the main one fails
            Regex::new(r"--+").unwrap_or_else(|_| {
                // Final fallback - use simple pattern that cannot fail
                Regex::new(r"").unwrap_or_else(|_| {
                    panic!("Critical: Even empty regex pattern is failing - regex engine corrupted")
                })
            })
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
        vm_println!("{}", MESSAGES.init_welcome);
        vm_println!();
        vm_println!("{}", MESSAGES.init_already_exists);
        vm_println!("   📁 {}", target_path.display());
        vm_println!();
        vm_println!("{}", MESSAGES.init_options_hint);
        vm_println!("   rm vm.yaml && vm init           # Start fresh");
        vm_println!("   vm init --file other.yaml      # Create elsewhere");
        vm_println!("   vm create                       # Use existing config");
        std::process::exit(1);
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
        vm_println!(
            "📝 Note: Directory name '{}' contains invalid characters for project names.",
            dir_name
        );
        vm_println!("   Using sanitized name: '{}'", sanitized_name);
        vm_println!();
    }

    // Load embedded defaults
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../../../configs/defaults.yaml");
    let mut config: VmConfig = serde_yaml::from_str(EMBEDDED_DEFAULTS)
        .map_err(|e| VmError::Serialization(format!("Failed to parse embedded defaults: {}", e)))?;

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
            // Parse the range string to get start and end
            if let Ok(range) = PortRange::parse(&range_str) {
                config.ports.range = Some(vec![range.start, range.end]);

                // Register this range
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

    // Smart service detection and configuration
    let services_to_configure = match services {
        Some(ref services_str) => {
            // Manual service specification
            services_str
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        }
        None => {
            // Smart detection
            detect_and_recommend_services(&current_dir)?
        }
    };

    // Apply service configurations
    for service in services_to_configure {
        // Load service config
        let service_path =
            crate::paths::resolve_tool_path(format!("configs/services/{}.yaml", service));
        if !service_path.exists() {
            vm_error!("Unknown service: {}", service);
            vm_error!("Available services: postgresql, redis, mongodb, docker");
            return Err(VmError::Config(
                "Service configuration not found".to_string(),
            ));
        }

        let service_config = VmConfig::from_file(&service_path).map_err(|e| {
            VmError::Config(format!("Failed to load service config: {}: {}", service, e))
        })?;

        // Extract only the specific service we want to enable from the service config
        if let Some(specific_service_config) = service_config.services.get(&service) {
            // Enable the specific service with its configuration
            let mut enabled_service = specific_service_config.clone();
            enabled_service.enabled = true;
            config.services.insert(service, enabled_service);
        }
    }

    // Apply port configuration
    if let Some(port_start) = ports {
        if port_start < 1024 {
            return Err(VmError::Config(format!(
                "Invalid port number: {} (must be >= 1024)",
                port_start
            )));
        }

        // Set up port range instead of individual ports - services will auto-assign
        config.ports.range = Some(vec![port_start, port_start + 9]);
    }

    // Convert config to Value and write with consistent formatting
    let config_yaml = serde_yaml::to_string(&config).map_err(|e| {
        VmError::Serialization(format!("Failed to serialize configuration to YAML: {}", e))
    })?;
    let config_value: Value = serde_yaml::from_str(&config_yaml).map_err(|e| {
        VmError::Serialization(format!("Failed to convert config to YAML Value: {}", e))
    })?;

    // Write using the centralized function for consistent formatting
    CoreOperations::write_yaml_file(&target_path, &config_value).map_err(|e| {
        VmError::Filesystem(format!(
            "Failed to write vm.yaml to {}: {}",
            target_path.display(),
            e
        ))
    })?;

    // Get the port range for display
    let port_display = if let Some(range) = &config.ports.range {
        format!("{}-{}", range[0], range[1])
    } else if let Some(port_start) = ports {
        format!("{}-{}", port_start, port_start + 9)
    } else {
        "auto".to_string()
    };

    // Clean success output
    vm_println!("{}", MESSAGES.init_welcome);
    vm_println!();
    vm_println!("✓ Initializing project: {}", sanitized_name);
    vm_println!("✓ Port range allocated: {}", port_display);
    if let Some(ref services_str) = services {
        vm_println!("✓ Services configured: {}", services_str);
    }
    vm_println!("✓ Configuration created: vm.yaml");
    vm_println!();
    vm_println!("{}", MESSAGES.init_success);
    vm_println!("{}", MESSAGES.init_next_steps);
    vm_println!("   vm create    # Launch your development environment");
    vm_println!("   vm --help    # View all available commands");
    vm_println!();
    vm_println!("📁 {}", target_path.display());

    Ok(())
}

/// Detect project technologies and recommend services
fn detect_and_recommend_services(project_dir: &std::path::Path) -> Result<Vec<String>> {
    use crate::detector::get_detected_technologies;

    let detected = get_detected_technologies(project_dir);

    if !detected.is_empty() {
        let services = get_recommended_services(&detected);

        // Show what was detected
        let detected_list: Vec<String> = detected.iter().cloned().collect();
        println!("🔍 Detected: {}", detected_list.join(", "));
        if !services.is_empty() {
            println!("✓ Services: {}", services.join(", "));
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

    for tech in detected_types {
        match tech.as_str() {
            "nodejs" | "react" | "vue" | "next" | "angular" => {
                if !services.contains(&"postgresql".to_string()) {
                    services.push("postgresql".to_string());
                }
            }
            "python" | "django" | "flask" => {
                if !services.contains(&"postgresql".to_string()) {
                    services.push("postgresql".to_string());
                }
                if !services.contains(&"redis".to_string()) {
                    services.push("redis".to_string());
                }
            }
            "rails" | "ruby" => {
                if !services.contains(&"postgresql".to_string()) {
                    services.push("postgresql".to_string());
                }
                if !services.contains(&"redis".to_string()) {
                    services.push("redis".to_string());
                }
            }
            "docker" => {
                if !services.contains(&"docker".to_string()) {
                    services.push("docker".to_string());
                }
            }
            _ => {}
        }
    }

    services
}
