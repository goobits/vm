// Configuration-related command handlers

use anyhow::Context;
use std::path::PathBuf;
use tracing::{debug, warn};

use crate::cli::{
    ConfigPresetSubcommand, ConfigProfileSubcommand, ConfigReadScope, ConfigSubcommand,
    ConfigWriteScope,
};
use crate::error::{VmError, VmResult};
use serde_yaml_ng as serde_yaml;
use vm_config::ports::{PortRange, PortRegistry};
use vm_config::validation::{validate_config, ValidationMode};
use vm_config::{config::VmConfig, AppConfig, ConfigOps};
use vm_core::msg;
use vm_core::{vm_print, vm_println, vm_progress, vm_success, vm_warning};
use vm_messages::messages::MESSAGES;

fn load_selected_config(
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<AppConfig> {
    AppConfig::load(config_path, profile, None).map_err(VmError::from)
}

/// Handle the `vm config validate` command.
fn handle_validate_command(config_path: Option<PathBuf>, profile: Option<String>) -> VmResult<()> {
    let path = config_path
        .clone()
        .map(Ok)
        .unwrap_or_else(find_project_config)?;
    ConfigOps::validate_file(&path).map_err(|error| match error {
        vm_core::error::VmError::Config(message) => VmError::validation(message, None::<String>),
        other => VmError::from(other),
    })?;
    let config = load_selected_config(config_path, profile)?.vm;
    if config.environments.is_empty() || config.provider.is_some() {
        let report = validate_config(&config, ValidationMode::Static).map_err(|error| {
            VmError::validation(
                format!("Unexpected configuration validation error: {error}"),
                None::<String>,
            )
        })?;

        if report.has_errors() {
            return Err(VmError::validation(
                format!("Configuration is invalid:\n{report}"),
                None::<String>,
            ));
        }
    }

    vm_success!("Configuration is valid.");
    Ok(())
}

/// Handle the `vm config show` command.
fn handle_show_command(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    scope: ConfigReadScope,
) -> VmResult<()> {
    let (mut value, source) = read_scope(scope, config_path.clone(), profile.clone())?;
    let origins = ConfigOrigins::load(scope, config_path, profile)?;
    let mut paths = Vec::new();
    collect_field_paths(&value, "", &mut paths);
    redact_yaml(&mut value, "");
    vm_println!("Config source: {source}");
    vm_println!(
        "\n---\n{}",
        serde_yaml::to_string(&value)
            .map_err(|e| { VmError::config(e, "Failed to serialize configuration to YAML") })?
    );
    vm_println!("Sources:");
    for path in paths {
        vm_println!("  {path}: {}", origins.source_for(&path));
    }
    Ok(())
}

struct ConfigOrigins {
    scope: ConfigReadScope,
    project_path: Option<PathBuf>,
    project: Option<serde_yaml::Value>,
    user_path: PathBuf,
    user: Option<serde_yaml::Value>,
    profile_name: Option<String>,
    preset_name: Option<String>,
    defaults: serde_yaml::Value,
    effective: Option<serde_yaml::Value>,
}

impl ConfigOrigins {
    fn load(
        scope: ConfigReadScope,
        config_path: Option<PathBuf>,
        profile: Option<String>,
    ) -> VmResult<Self> {
        let user_path = vm_core::user_paths::global_config_path()?;
        let user = if scope == ConfigReadScope::Project {
            None
        } else {
            user_path
                .is_file()
                .then(|| read_raw_config(user_path.clone()).map(|(value, _)| value))
                .transpose()?
        };
        let project_path = if scope == ConfigReadScope::User {
            None
        } else {
            match config_path {
                Some(path) => Some(path),
                None => find_project_config().ok(),
            }
        };
        let project = project_path
            .as_ref()
            .filter(|path| path.is_file())
            .map(|path| read_raw_config(path.clone()).map(|(value, _)| value))
            .transpose()?;
        let loaded = if scope == ConfigReadScope::Effective {
            project_path
                .as_ref()
                .filter(|path| path.is_file())
                .map(|path| VmConfig::load(Some(path.clone())))
                .transpose()?
        } else {
            None
        };
        let profile_name = loaded
            .as_ref()
            .and_then(|config| AppConfig::resolve_profile_name(config, profile.as_deref(), None));
        let preset_name = project
            .as_ref()
            .and_then(|value| nested_value(value, "preset"))
            .and_then(serde_yaml::Value::as_str)
            .map(str::to_string);
        let defaults = serde_yaml::to_value(VmConfig::default())
            .map_err(|error| VmError::config(error, "Cannot serialize default configuration"))?;
        let effective = if scope == ConfigReadScope::Effective {
            Some(read_scope(scope, project_path.clone(), profile)?.0)
        } else {
            None
        };
        Ok(Self {
            scope,
            project_path,
            project,
            user_path,
            user,
            profile_name,
            preset_name,
            defaults,
            effective,
        })
    }

    fn source_for(&self, field: &str) -> String {
        match self.scope {
            ConfigReadScope::Project => {
                return self
                    .project_path
                    .as_ref()
                    .map_or_else(|| "project".to_string(), |path| path.display().to_string())
            }
            ConfigReadScope::User => return self.user_path.display().to_string(),
            ConfigReadScope::Effective => {}
        }
        if let (Some(profile), Some(project)) = (&self.profile_name, &self.project) {
            let profile_field = format!("profiles.{profile}.{field}");
            if nested_value(project, &profile_field).is_some() {
                return format!(
                    "profile {profile} in {}",
                    self.project_path.as_ref().unwrap().display()
                );
            }
        }
        if self
            .project
            .as_ref()
            .is_some_and(|project| nested_value(project, field).is_some())
        {
            return self.project_path.as_ref().unwrap().display().to_string();
        }
        if let Some(name) = field
            .strip_prefix("tools.")
            .and_then(|tail| tail.split('.').next())
        {
            if self
                .project
                .as_ref()
                .is_some_and(|project| nested_value(project, &format!("tools.{name}")).is_some())
            {
                return self.project_path.as_ref().unwrap().display().to_string();
            }
        }
        if field.starts_with("tools.")
            && self
                .user
                .as_ref()
                .is_some_and(|user| nested_value(user, field).is_some())
        {
            return self.user_path.display().to_string();
        }
        if let Some(preset) = &self.preset_name {
            if nested_value(&self.defaults, field)
                != self
                    .effective
                    .as_ref()
                    .and_then(|value| nested_value(value, field))
            {
                return format!("preset {preset}");
            }
        }
        "built-in defaults".to_string()
    }
}

fn collect_field_paths(value: &serde_yaml::Value, prefix: &str, paths: &mut Vec<String>) {
    if let Some(fields) = value.as_mapping() {
        for (key, child) in fields {
            let Some(key) = key.as_str() else { continue };
            let path = if prefix.is_empty() {
                key.to_string()
            } else {
                format!("{prefix}.{key}")
            };
            if child.as_mapping().is_some_and(|map| !map.is_empty()) {
                collect_field_paths(child, &path, paths);
            } else {
                paths.push(path);
            }
        }
    }
}

fn read_scope(
    scope: ConfigReadScope,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<(serde_yaml::Value, String)> {
    match scope {
        ConfigReadScope::Effective => {
            let config = load_selected_config(config_path, profile)?.vm;
            let source = config.source_path.as_ref().map_or_else(
                || "built-in defaults".to_string(),
                |path| {
                    format!(
                        "{} (merged with user settings and defaults)",
                        path.display()
                    )
                },
            );
            Ok((
                serde_yaml::to_value(config)
                    .map_err(|e| VmError::config(e, "Failed to serialize configuration"))?,
                source,
            ))
        }
        ConfigReadScope::Project => {
            let path = config_path.map(Ok).unwrap_or_else(find_project_config)?;
            read_raw_config(path)
        }
        ConfigReadScope::User => {
            let path = vm_core::user_paths::global_config_path()?;
            read_raw_config(path)
        }
    }
}

fn find_project_config() -> VmResult<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let path = dir.join("vm.yaml");
        if path.is_file() {
            return Ok(path);
        }
        if !dir.pop() {
            break;
        }
    }
    Err(VmError::validation(
        "No project configuration found; run inside a project or select --scope user",
        None::<String>,
    ))
}

fn read_raw_config(path: PathBuf) -> VmResult<(serde_yaml::Value, String)> {
    let content = std::fs::read_to_string(&path).map_err(|e| {
        VmError::config(
            e,
            format!("Cannot read configuration at {}", path.display()),
        )
    })?;
    let value = serde_yaml::from_str(&content)
        .map_err(|e| VmError::config(e, format!("Invalid configuration at {}", path.display())))?;
    Ok((value, path.display().to_string()))
}

fn sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    [
        "token",
        "password",
        "secret",
        "credential",
        "api_key",
        "private_key",
    ]
    .iter()
    .any(|part| lower.contains(part))
}

fn redact_yaml(value: &mut serde_yaml::Value, key: &str) {
    if sensitive_key(key) {
        *value = serde_yaml::Value::String("[redacted]".to_string());
        return;
    }
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (key, child) in map.iter_mut() {
                redact_yaml(child, key.as_str().unwrap_or(""));
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for child in items {
                redact_yaml(child, key);
            }
        }
        _ => {}
    }
}

fn handle_get_command(
    field: &str,
    scope: ConfigReadScope,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let (value, _) = read_scope(scope, config_path.clone(), profile.clone())?;
    let origins = ConfigOrigins::load(scope, config_path, profile)?;
    let mut selected = nested_value(&value, field)
        .ok_or_else(|| {
            VmError::validation(
                format!("Unknown configuration field: {field}"),
                None::<String>,
            )
        })?
        .clone();
    redact_yaml(&mut selected, field);
    vm_println!(
        "{}",
        serde_yaml::to_string(&selected)
            .map_err(|e| VmError::config(e, "Failed to serialize configuration field"))?
    );
    vm_println!("Source: {}", origins.source_for(field));
    Ok(())
}

fn nested_value<'a>(value: &'a serde_yaml::Value, field: &str) -> Option<&'a serde_yaml::Value> {
    field.split('.').try_fold(value, |current, part| {
        current
            .as_mapping()?
            .get(serde_yaml::Value::String(part.to_string()))
    })
}

fn report_unset_effective(
    field: &str,
    _scope: ConfigWriteScope,
    config_path: Option<PathBuf>,
    profile: Option<String>,
) {
    let Ok((value, _)) = read_scope(
        ConfigReadScope::Effective,
        config_path.clone(),
        profile.clone(),
    ) else {
        vm_println!("Effective {field}: unavailable outside a project");
        return;
    };
    let Some(value) = nested_value(&value, field) else {
        vm_println!("Effective {field}: unset");
        return;
    };
    let mut value = value.clone();
    redact_yaml(&mut value, field);
    if let Ok(serialized) = serde_yaml::to_string(&value) {
        vm_println!("Effective {field}: {}", serialized.trim());
        if let Ok(origins) = ConfigOrigins::load(ConfigReadScope::Effective, config_path, profile) {
            vm_println!("Source: {}", origins.source_for(field));
        }
    }
}

fn handle_render_command(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    instance: Option<&str>,
) -> VmResult<()> {
    let app_config = load_selected_config(config_path, profile)?;
    let config = app_config.vm;
    let provider = config.provider.as_deref().unwrap_or("docker");
    if !matches!(provider, "docker" | "podman") {
        return Err(VmError::validation(
            format!("Provider '{provider}' does not generate Docker Compose"),
            None::<String>,
        ));
    }

    let report = validate_config(&config, ValidationMode::Static).map_err(|error| {
        VmError::validation(
            format!("Unexpected configuration validation error: {error}"),
            None::<String>,
        )
    })?;
    if report.has_errors() {
        return Err(VmError::validation(report.to_string(), None::<String>));
    }

    let project_dir = config.project_dir()?;
    let context = vm_provider::ProviderContext::default().with_config(app_config.global);
    let rendered = vm_provider::render_compose_preview(&config, &project_dir, instance, &context)?;
    vm_print!("{rendered}");
    Ok(())
}

fn handle_profile_list(config_path: Option<PathBuf>) -> VmResult<()> {
    let config = VmConfig::load(config_path)?;
    let profiles = match config.profiles {
        Some(profiles) if !profiles.is_empty() => profiles,
        _ => {
            vm_println!("No profiles defined in vm.yaml.");
            return Ok(());
        }
    };

    let mut names: Vec<String> = profiles.keys().cloned().collect();
    names.sort();

    let default_profile = config.default_profile.as_deref();

    vm_println!("Profiles:");
    for name in names {
        if Some(name.as_str()) == default_profile {
            vm_println!("  * {}", name);
        } else {
            vm_println!("  - {}", name);
        }
    }

    Ok(())
}

fn handle_profile_show(name: &str, config_path: Option<PathBuf>) -> VmResult<()> {
    let config = VmConfig::load(config_path)?;
    let mut value = serde_yaml::to_value(
        config
            .profiles
            .as_ref()
            .and_then(|p| p.get(name))
            .ok_or_else(|| {
                VmError::validation(format!("Unknown profile: {name}"), None::<String>)
            })?,
    )
    .map_err(|e| VmError::config(e, "Failed to serialize profile"))?;
    redact_yaml(&mut value, "");
    vm_println!(
        "{}",
        serde_yaml::to_string(&value)
            .map_err(|e| VmError::config(e, "Failed to serialize profile"))?
    );
    Ok(())
}

fn handle_profile_set(name: &str, config_path: Option<PathBuf>) -> VmResult<()> {
    let config = VmConfig::load(config_path.clone()).map_err(VmError::from)?;
    let has_profile = config
        .profiles
        .as_ref()
        .map(|profiles| profiles.contains_key(name))
        .unwrap_or(false);

    if !has_profile {
        return Err(VmError::config(
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Profile '{}' not found in vm.yaml", name),
            ),
            "Invalid default profile",
        ));
    }

    let values = vec![name.to_string()];
    ConfigOps::set_at("default_profile", &values, false, false, config_path).map_err(VmError::from)
}

/// Handle configuration management commands
pub fn handle_config_command(
    command: &ConfigSubcommand,
    profile: Option<String>,
    config_path: Option<PathBuf>,
) -> VmResult<()> {
    match command {
        ConfigSubcommand::Validate => handle_validate_command(config_path, profile),
        ConfigSubcommand::Show { scope } => handle_show_command(config_path, profile, *scope),
        ConfigSubcommand::Render { env } => {
            handle_render_command(config_path, profile, env.as_deref())
        }
        ConfigSubcommand::Set {
            field,
            values,
            value_json,
            scope,
        } => {
            let path = project_write_path(*scope, config_path)?;
            if let Some(json) = value_json {
                Ok(ConfigOps::set_json_at(
                    field,
                    json,
                    *scope == ConfigWriteScope::User,
                    path,
                )?)
            } else {
                Ok(ConfigOps::set_at(
                    field,
                    values,
                    *scope == ConfigWriteScope::User,
                    false,
                    path,
                )?)
            }
        }
        ConfigSubcommand::Get { field, scope } => {
            handle_get_command(field, *scope, config_path, profile)
        }
        ConfigSubcommand::Unset { field, scope } => {
            let path = project_write_path(*scope, config_path.clone())?;
            ConfigOps::unset_at(field, *scope == ConfigWriteScope::User, path)?;
            report_unset_effective(field, *scope, config_path, profile);
            Ok(())
        }
        ConfigSubcommand::Presets { command } => match command {
            ConfigPresetSubcommand::List => {
                Ok(ConfigOps::preset_at("", false, true, None, config_path)?)
            }
            ConfigPresetSubcommand::Show { name } => Ok(ConfigOps::preset_at(
                "",
                false,
                false,
                Some(name),
                config_path,
            )?),
            ConfigPresetSubcommand::Apply { names, scope } => {
                let path = project_write_path(*scope, config_path)?;
                Ok(ConfigOps::preset_at(
                    &names.join(","),
                    *scope == ConfigWriteScope::User,
                    false,
                    None,
                    path,
                )?)
            }
        },
        ConfigSubcommand::Profiles { command } => match command {
            ConfigProfileSubcommand::List => handle_profile_list(config_path),
            ConfigProfileSubcommand::Show { name } => handle_profile_show(name, config_path),
            ConfigProfileSubcommand::SetDefault { name } => {
                let path = project_write_path(ConfigWriteScope::Project, config_path)?;
                handle_profile_set(name, path)
            }
        },
        ConfigSubcommand::Ports { fix } => handle_ports_command(*fix),
    }
}

fn project_write_path(
    scope: ConfigWriteScope,
    config_path: Option<PathBuf>,
) -> VmResult<Option<PathBuf>> {
    if scope == ConfigWriteScope::Project {
        let path = config_path.map(Ok).unwrap_or_else(find_project_config)?;
        if !path.is_file() {
            return Err(VmError::validation(
                format!("Project configuration does not exist: {}", path.display()),
                None::<String>,
            ));
        }
        return Ok(Some(path));
    }
    Ok(None)
}

/// Handle ports command
pub fn handle_ports_command(fix: bool) -> VmResult<()> {
    debug!("Handling ports command: fix={}", fix);

    // Load current project configuration
    let config = VmConfig::load(None)?;

    // Get project name
    let project_name = config
        .project
        .as_ref()
        .and_then(|p| p.name.as_ref())
        .context("No project name found in configuration")?;

    // Get current port range from config
    let current_port_range = config
        .ports
        .range
        .as_ref()
        .and_then(|range| {
            if range.len() == 2 {
                Some(format!("{}-{}", range[0], range[1]))
            } else {
                None
            }
        })
        .context("No port range found in configuration")?;

    vm_println!(
        "{}",
        msg!(
            MESSAGES.config.ports_header,
            project = project_name,
            range = &current_port_range
        )
    );

    if !fix {
        // For basic ports command, just show the configuration
        return Ok(());
    }

    // Parse current range
    let current_range =
        PortRange::parse(&current_port_range).context("Failed to parse current port range")?;

    // Only check for conflicts when --fix is specified
    vm_progress!("{}", MESSAGES.config.ports_checking);

    // Check for conflicts with running Docker containers
    let executable = config.provider.as_deref().unwrap_or("docker");
    let conflicts = check_docker_port_conflicts(executable, &current_range)?;

    if conflicts.is_empty() {
        vm_success!("No port conflicts detected");
        return Ok(());
    }

    warn!("Port conflicts detected:");
    for conflict in &conflicts {
        vm_warning!("Port {} is in use by {}", conflict.port, conflict.container);
    }

    // Fix conflicts by finding a new port range
    vm_progress!("{}", MESSAGES.config.ports_fixing);

    // Calculate range size from current range
    let range_size = current_range.size();

    let current_dir = std::env::current_dir()?;
    let mut registry = PortRegistry::load().context("Failed to load port registry")?;
    let new_range = registry
        .replace_with_next_range(
            project_name,
            &current_range,
            range_size,
            3000,
            &current_dir.to_string_lossy(),
        )
        .context("Failed to reserve a replacement port range")?;
    let new_range_str = new_range.to_string();

    vm_println!(
        "{}",
        msg!(MESSAGES.config.ports_updated, range = &new_range_str)
    );

    // Update vm.yaml with new port range
    update_vm_config_ports(&new_range_str)?;

    vm_println!(
        "{}",
        msg!(
            MESSAGES.config.ports_resolved,
            old = &current_port_range,
            new = &new_range_str
        )
    );
    vm_println!("{}", MESSAGES.config.ports_restart_hint);

    Ok(())
}

#[derive(Debug)]
struct PortConflict {
    port: u16,
    container: String,
}

/// Check for conflicts between the given port range and running Docker containers
fn check_docker_port_conflicts(executable: &str, range: &PortRange) -> VmResult<Vec<PortConflict>> {
    use std::process::Command;

    let mut conflicts = Vec::new();

    // Run docker ps to get running containers with port mappings
    let output = Command::new(executable)
        .args(["ps", "--format", "{{.Names}}:{{.Ports}}"])
        .output()
        .context("Failed to run docker ps command")?;

    if !output.status.success() {
        return Err(VmError::general(
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "Docker command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            ),
            "Failed to check Docker port conflicts",
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let Some((container, ports)) = line.split_once(':') else {
            continue;
        };

        // Parse port mappings like "0.0.0.0:3010->3010/tcp"
        for port_mapping in ports.split(", ") {
            let Some(host_port) = extract_host_port(port_mapping) else {
                continue;
            };

            if host_port >= range.start && host_port <= range.end {
                conflicts.push(PortConflict {
                    port: host_port,
                    container: container.to_string(),
                });
            }
        }
    }

    Ok(conflicts)
}

/// Extract host port from Docker port mapping string
fn extract_host_port(port_mapping: &str) -> Option<u16> {
    // Handle formats like:
    // "0.0.0.0:3010->3010/tcp"
    // "[::]:3010->3010/tcp"
    // "3010->3010/tcp"

    if let Some(arrow_pos) = port_mapping.find("->") {
        let host_part = &port_mapping[..arrow_pos];

        // Extract port from host part
        if let Some(colon_pos) = host_part.rfind(':') {
            let port_str = &host_part[colon_pos + 1..];
            port_str.parse().ok()
        } else {
            // Direct port mapping without host
            host_part.parse().ok()
        }
    } else {
        None
    }
}

/// Update vm.yaml with new port range
fn update_vm_config_ports(new_range: &str) -> VmResult<()> {
    use std::fs;

    let config_path = std::env::current_dir()?.join("vm.yaml");

    if !config_path.exists() {
        return Err(VmError::filesystem(
            std::io::Error::new(std::io::ErrorKind::NotFound, "vm.yaml not found"),
            "vm.yaml",
            "update configuration",
        ));
    }

    let content = fs::read_to_string(&config_path).context("Failed to read vm.yaml")?;

    // Parse YAML
    let mut yaml: serde_yaml::Value =
        serde_yaml::from_str(&content).context("Failed to parse vm.yaml")?;

    // Update port_range field
    if let Some(mapping) = yaml.as_mapping_mut() {
        mapping.insert(
            serde_yaml::Value::String("port_range".to_string()),
            serde_yaml::Value::String(new_range.to_string()),
        );

        // Also update individual port mappings if they exist
        if let Some(ports) = mapping.get_mut(serde_yaml::Value::String("ports".to_string())) {
            if let Some(ports_map) = ports.as_mapping_mut() {
                let range = PortRange::parse(new_range)?;
                let start_port = range.start;

                // Update backend port (first port in range)
                if ports_map.contains_key(serde_yaml::Value::String("backend".to_string())) {
                    ports_map.insert(
                        serde_yaml::Value::String("backend".to_string()),
                        serde_yaml::Value::Number(serde_yaml::Number::from(start_port)),
                    );
                }

                // Update frontend port (second port in range)
                if ports_map.contains_key(serde_yaml::Value::String("frontend".to_string())) {
                    ports_map.insert(
                        serde_yaml::Value::String("frontend".to_string()),
                        serde_yaml::Value::Number(serde_yaml::Number::from(start_port + 1)),
                    );
                }
            }
        }
    }

    // Write back to file
    let updated_content =
        serde_yaml::to_string(&yaml).context("Failed to serialize updated YAML")?;

    fs::write(&config_path, updated_content).context("Failed to write updated vm.yaml")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        handle_config_command, handle_validate_command, load_selected_config, redact_yaml,
    };
    use crate::cli::{ConfigSubcommand, ConfigWriteScope};

    #[test]
    fn validation_honors_explicit_config_and_does_not_modify_it() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        let contents = b"project:\n  name: selected\nprovider: docker\n";
        std::fs::write(&config_path, contents).unwrap();

        handle_validate_command(Some(config_path.clone()), None).unwrap();

        assert_eq!(std::fs::read(config_path).unwrap(), contents);
    }

    #[test]
    fn validation_rejects_misspelled_nested_fields() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("vm.yaml");
        let contents = "project:\n  name: selected\nprovider: docker\nvm:\n  memroy: 4096\n";
        std::fs::write(&config_path, contents).unwrap();
        let error = handle_validate_command(Some(config_path.clone()), None).unwrap_err();
        assert!(error
            .to_string()
            .contains("Unknown configuration field: vm.memroy"));
        assert_eq!(std::fs::read_to_string(config_path).unwrap(), contents);
    }

    #[test]
    fn selected_profile_is_loaded_from_explicit_config() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        std::fs::write(
            &config_path,
            r#"
project:
  name: base
provider: docker
profiles:
  feature:
    project:
      name: feature
"#,
        )
        .unwrap();

        let loaded = load_selected_config(Some(config_path), Some("feature".to_string())).unwrap();
        assert_eq!(
            loaded
                .vm
                .project
                .and_then(|project| project.name)
                .as_deref(),
            Some("feature")
        );
    }

    #[test]
    fn redaction_covers_nested_sensitive_fields() {
        let mut value: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            "packages:\n  auth_token: private\nitems:\n  - credentials:\n      password: hidden\nname: visible\n",
        )
        .unwrap();
        redact_yaml(&mut value, "");
        let output = serde_yaml_ng::to_string(&value).unwrap();
        assert!(!output.contains("private"));
        assert!(!output.contains("hidden"));
        assert!(output.contains("visible"));
    }

    #[test]
    fn project_write_uses_explicit_config_path() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("selected.yaml");
        std::fs::write(
            &config_path,
            "project:\n  name: selected\nprovider: docker\n",
        )
        .unwrap();
        handle_config_command(
            &ConfigSubcommand::Set {
                field: "vm.memory".to_string(),
                values: vec!["4096".to_string()],
                value_json: None,
                scope: ConfigWriteScope::Project,
            },
            None,
            Some(config_path.clone()),
        )
        .unwrap();
        let content = std::fs::read_to_string(config_path).unwrap();
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&content).unwrap();
        assert_eq!(value["vm"]["memory"].as_str(), Some("4096"));
    }

    #[test]
    fn explicit_project_write_requires_existing_config() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("missing.yaml");
        let result = handle_config_command(
            &ConfigSubcommand::Set {
                field: "vm.memory".to_string(),
                values: vec!["4096".to_string()],
                value_json: None,
                scope: ConfigWriteScope::Project,
            },
            None,
            Some(config_path.clone()),
        );
        assert!(result.is_err());
        assert!(!config_path.exists());
    }
}
