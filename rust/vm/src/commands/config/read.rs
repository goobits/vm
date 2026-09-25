// Configuration-related command handlers

use std::path::PathBuf;

use crate::cli::{ConfigReadScope, ConfigWriteScope};
use crate::error::{VmError, VmResult};
use serde_yaml_ng as serde_yaml;
use vm_config::validation::{validate_config, ValidationMode};
use vm_config::{config::VmConfig, AppConfig, ConfigOps};
use vm_core::{vm_print, vm_println, vm_success};

mod origins;
use origins::ConfigOrigins;

pub(super) fn load_selected_config(
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<AppConfig> {
    AppConfig::load(config_path, profile, None).map_err(VmError::from)
}

/// Handle the `vm config validate` command.
pub(super) fn handle_validate_command(
    config_path: Option<PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
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
pub(super) fn handle_show_command(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    scope: ConfigReadScope,
    json: bool,
) -> VmResult<()> {
    let (mut value, source) = read_scope(scope, config_path.clone(), profile.clone())?;
    let origins = ConfigOrigins::load(scope, config_path, profile)?;
    let mut paths = Vec::new();
    collect_field_paths(&value, "", &mut paths);
    redact_yaml(&mut value, "");
    if json {
        let sources = paths
            .into_iter()
            .map(|path| {
                let source = origins.source_for(&path);
                (path, source)
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        return crate::presentation::success(
            "config show",
            serde_json::json!({"config": value, "sources": sources}),
        );
    }
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

pub(super) fn find_project_config() -> VmResult<PathBuf> {
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

pub(super) fn redact_yaml(value: &mut serde_yaml::Value, key: &str) {
    if sensitive_key(key) {
        *value = serde_yaml::Value::String("<redacted>".to_string());
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

pub(super) fn handle_get_command(
    field: &str,
    scope: ConfigReadScope,
    config_path: Option<PathBuf>,
    profile: Option<String>,
    json: bool,
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
    if json {
        return crate::presentation::success(
            "config get",
            serde_json::json!({
                "field": field,
                "value": selected,
                "source": origins.source_for(field),
            }),
        );
    }
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

pub(super) fn report_unset_effective(
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

pub(super) fn handle_render_command(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<&str>,
) -> VmResult<()> {
    let app_config = load_selected_config(config_path, profile)?;
    let (config, instance) = select_render_environment(app_config.vm, environment)?;
    let provider = config.provider.as_deref().unwrap_or("docker");

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
    let rendered = match provider {
        "docker" | "podman" => vm_provider::render_compose_preview(
            &config,
            &project_dir,
            instance.as_deref(),
            &context,
        )?,
        "tart" => {
            #[cfg(any(target_os = "macos", feature = "tart"))]
            {
                vm_provider::render_tart_preview(&config, instance.as_deref())?
            }
            #[cfg(not(any(target_os = "macos", feature = "tart")))]
            {
                return Err(VmError::validation(
                    "This binary was built without Tart support",
                    None::<String>,
                ));
            }
        }
        _ => {
            return Err(VmError::validation(
                format!("Provider '{provider}' cannot render configuration"),
                None::<String>,
            ))
        }
    };
    vm_print!("{rendered}");
    Ok(())
}

fn select_render_environment(
    config: VmConfig,
    requested: Option<&str>,
) -> VmResult<(VmConfig, Option<String>)> {
    if config.environments.is_empty() {
        if let Some(requested) = requested {
            return Err(VmError::validation(
                format!("Environment '{requested}' is not declared"),
                None::<String>,
            ));
        }
        return Ok((config, None));
    }
    let selected = crate::commands::declarations::selected_name(&config, requested)?
        .expect("declared environment selection has a name");
    let declaration = config.environments.get(&selected).ok_or_else(|| {
        VmError::validation(
            format!("Environment '{selected}' is not declared"),
            None::<String>,
        )
    })?;
    Ok((declaration.apply_to(&config), Some(selected)))
}

pub(super) fn handle_profile_list(config_path: Option<PathBuf>) -> VmResult<()> {
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

pub(super) fn handle_profile_show(name: &str, config_path: Option<PathBuf>) -> VmResult<()> {
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

pub(super) fn handle_profile_set(name: &str, config_path: Option<PathBuf>) -> VmResult<()> {
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
