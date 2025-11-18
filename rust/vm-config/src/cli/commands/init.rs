// Standard library imports
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// External crate imports
use regex::Regex;
use serde_yaml::Value;
use serde_yaml_ng as serde_yaml;
use tracing::{error, info, warn};
use vm_core::error::{Result, VmError};

// Internal crate imports
use vm_messages::messages::MESSAGES;

// Local module imports
use crate::config::VmConfig;
use crate::ports::{PortRange, PortRegistry};
use crate::preset::PresetDetector;
use crate::yaml::core::CoreOperations;
use vm_plugin::PresetCategory;

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

pub fn execute(
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
        std::process::exit(1);
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
    info!("   rm vm.yaml && vm init           # Start fresh");
    info!("   vm init --file other.yaml      # Create elsewhere");
    info!("   vm create                       # Use existing config");
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
    const EMBEDDED_DEFAULTS: &str = include_str!("../../../../../configs/defaults.yaml");
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
    use crate::detector::os::detect_host_os;

    if let Some(ref mut vm) = config.vm {
        if vm.swap.is_none() || vm.swappiness.is_none() {
            let host_os = detect_host_os();

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

/// Build config from a preset
fn build_config_from_preset(sanitized_name: &str, preset_name: &str) -> Result<VmConfig> {
    use crate::paths;
    use crate::preset::PresetDetector;

    // Load the preset
    let presets_dir = paths::get_presets_dir();
    let project_dir = std::env::current_dir()?;
    let detector = PresetDetector::new(project_dir, presets_dir);

    // Check if preset exists (using list_all_presets to include box presets)
    let available_presets = detector.list_all_presets()?;
    if !available_presets.contains(&preset_name.to_string()) {
        return Err(VmError::Config(format!(
            "Preset '{}' not found. Available presets: {}",
            preset_name,
            available_presets.join(", ")
        )));
    }

    // Get preset category
    let preset_category = get_preset_category(&detector, preset_name)?;

    match preset_category {
        PresetCategory::Box => {
            // For box presets, create minimal config with just box reference
            info!("🎁 Using box preset '{}'", preset_name);
            build_minimal_box_config(sanitized_name, preset_name, &detector)
        }
        PresetCategory::Provision => {
            // For provision presets, merge packages and services
            info!("📦 Using provision preset '{}'", preset_name);
            build_config_with_provision_preset(sanitized_name, preset_name, &detector)
        }
    }
}

/// Get the category of a preset
fn get_preset_category(detector: &PresetDetector, preset_name: &str) -> Result<PresetCategory> {
    // Try to get category from plugin metadata
    if let Ok(plugins) = vm_plugin::discover_plugins() {
        for plugin in plugins {
            if plugin.info.plugin_type == vm_plugin::PluginType::Preset
                && plugin.info.name == preset_name
            {
                // Check if plugin info has preset_category
                if let Some(category) = &plugin.info.preset_category {
                    return Ok(category.clone());
                }
                // If not specified in plugin metadata, try preset content
                if let Ok(content) = vm_plugin::load_preset_content(&plugin) {
                    return Ok(content.category.clone());
                }
            }
        }
    }

    // Fallback: if preset has vm_box field, assume it's a box preset
    let preset_config = detector.load_preset(preset_name)?;
    if preset_config
        .vm
        .as_ref()
        .and_then(|vm| vm.r#box.as_ref())
        .is_some()
    {
        Ok(PresetCategory::Box)
    } else {
        Ok(PresetCategory::Provision)
    }
}

/// Build minimal config for box preset
fn build_minimal_box_config(
    sanitized_name: &str,
    preset_name: &str,
    detector: &PresetDetector,
) -> Result<VmConfig> {
    // Load preset to get box reference
    let preset_config = detector.load_preset(preset_name)?;

    // Start with default config
    let mut config = build_initial_config(sanitized_name)?;

    // Copy only the box reference from preset
    if let Some(preset_vm) = preset_config.vm {
        if let Some(box_spec) = preset_vm.r#box {
            if config.vm.is_none() {
                config.vm = Some(crate::config::VmSettings::default());
            }
            if let Some(vm_settings) = config.vm.as_mut() {
                vm_settings.r#box = Some(box_spec);
            }
        }
    }

    // Merge preset configuration (networking, aliases, terminal, host_sync, etc.)
    // Don't merge: packages, versions (they're in the box)
    if let Some(networking) = preset_config.networking {
        config.networking = Some(networking);
    }
    if !preset_config.aliases.is_empty() {
        config.aliases = preset_config.aliases;
    }
    if let Some(terminal) = preset_config.terminal {
        config.terminal = Some(terminal);
    }
    if let Some(host_sync) = preset_config.host_sync {
        config.host_sync = Some(host_sync);
    }

    // Clear packages/versions - they come from the box/preset at runtime
    config.versions = None;
    config.apt_packages.clear();
    config.npm_packages.clear();
    config.pip_packages.clear();
    config.cargo_packages.clear();

    Ok(config)
}

/// Build config with provision preset merged in
fn build_config_with_provision_preset(
    sanitized_name: &str,
    preset_name: &str,
    detector: &PresetDetector,
) -> Result<VmConfig> {
    use crate::merge::ConfigMerger;

    // Start with default config
    let base_config = build_initial_config(sanitized_name)?;

    // Load and merge preset
    let preset_config = detector.load_preset(preset_name)?;
    let mut merged_config = ConfigMerger::new(base_config).merge(preset_config)?;

    // Set preset reference
    merged_config.preset = Some(preset_name.to_string());

    Ok(merged_config)
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
        // Try to load service config from file, or use embedded defaults
        let service_path =
            crate::paths::resolve_tool_path(format!("configs/services/{service}.yaml"));

        let service_config = if service_path.exists() {
            VmConfig::from_file(&service_path).map_err(|e| {
                VmError::Config(format!("Failed to load service config: {service}: {e}"))
            })?
        } else {
            // Use embedded default configurations
            let default_config = match service.as_str() {
                "postgresql" => include_str!("../../../resources/services/postgresql.yaml"),
                "redis" => include_str!("../../../resources/services/redis.yaml"),
                "mongodb" => include_str!("../../../resources/services/mongodb.yaml"),
                "docker" => include_str!("../../../resources/services/docker.yaml"),
                _ => {
                    error!("Unknown service: {}", service);
                    error!("Available services: postgresql, redis, mongodb, docker");
                    return Err(VmError::Config(
                        "Service configuration not found".to_string(),
                    ));
                }
            };

            crate::yaml::CoreOperations::parse_yaml_with_diagnostics(
                default_config,
                &format!("embedded service config for {}", service),
            )?
        };

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
    info!("   vm create    # Launch your development environment");
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
