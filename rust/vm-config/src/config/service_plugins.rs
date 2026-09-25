//! Resolve activated, declarative service plugins before provider effects.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use vm_core::error::{Result, VmError};
use vm_plugin::{load_installed_service, load_service_content, validate_plugin, Plugin};

use super::{ServiceConfig, VmConfig};

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedServicePlugin {
    pub name: String,
    pub image: String,
    pub ports: Vec<PluginPort>,
    pub volumes: Vec<PluginVolume>,
    pub environment: Vec<(String, String)>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginPort {
    pub host: u16,
    pub container: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginVolume {
    pub name: String,
    pub target: String,
}

pub fn resolve_service_plugins(config: &VmConfig) -> Result<Vec<ResolvedServicePlugin>> {
    resolve_with(config, load_installed_service)
}

fn resolve_with(
    config: &VmConfig,
    load: impl Fn(&str) -> anyhow::Result<Plugin>,
) -> Result<Vec<ResolvedServicePlugin>> {
    let active = config
        .services
        .iter()
        .filter(|(_, service)| service.enabled && service.plugin.is_some())
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(Vec::new());
    }
    if !config
        .provider
        .as_ref()
        .is_some_and(super::ProviderName::is_container)
    {
        return Err(VmError::Config(
            "Service plugins require the Docker or Podman provider".into(),
        ));
    }
    let mut used_ports = HashMap::<u16, String>::new();
    for mapping in &config.ports.mappings {
        used_ports.insert(mapping.host, "application port mapping".into());
    }
    for (name, service) in &config.services {
        if service.enabled && service.plugin.is_none() {
            if let Some(port) = service.port {
                used_ports.insert(port, format!("built-in service '{name}'"));
            }
        }
    }
    let mut resolved = Vec::new();
    for (name, settings) in active {
        validate_declaration(name, settings)?;
        let plugin_name = settings
            .plugin
            .as_deref()
            .expect("filtered activated plugin");
        let plugin = load(plugin_name).map_err(|error| {
            VmError::Config(format!(
                "Cannot activate service plugin '{plugin_name}': {error}"
            ))
        })?;
        let validation =
            validate_plugin(&plugin).map_err(|error| VmError::Config(error.to_string()))?;
        if !validation.is_valid {
            let errors = validation
                .errors
                .iter()
                .map(|error| format!("{}: {}", error.field, error.message))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(VmError::Config(format!(
                "Invalid service plugin '{plugin_name}': {errors}"
            )));
        }
        let content =
            load_service_content(&plugin).map_err(|error| VmError::Config(error.to_string()))?;
        let mut ports = Vec::new();
        for mapping in content.ports {
            let (host, container) = mapping.split_once(':').unwrap_or((&mapping, &mapping));
            let host = host.parse::<u16>().expect("validated port");
            let container = container.parse::<u16>().expect("validated port");
            if let Some(other) = used_ports.insert(host, format!("service plugin '{name}'")) {
                return Err(VmError::Config(format!(
                    "Service plugin '{name}' host port {host} conflicts with {other}"
                )));
            }
            if config
                .ports
                .range
                .as_ref()
                .is_some_and(|range| range.len() == 2 && (range[0]..=range[1]).contains(&host))
            {
                return Err(VmError::Config(format!("Service plugin '{name}' host port {host} conflicts with application port range")));
            }
            ports.push(PluginPort { host, container });
        }
        let volumes = content
            .volumes
            .into_iter()
            .map(|mapping| {
                let (logical, target) = mapping.split_once(':').expect("validated volume");
                PluginVolume {
                    name: format!("plugin_{name}_{logical}"),
                    target: target.to_string(),
                }
            })
            .collect();
        let mut environment = content.environment.into_iter().collect::<Vec<_>>();
        environment.sort_by(|left, right| left.0.cmp(&right.0));
        resolved.push(ResolvedServicePlugin {
            name: name.to_string(),
            image: content.image,
            ports,
            volumes,
            environment,
            depends_on: content.depends_on,
        });
    }
    validate_dependencies(config, &resolved)?;
    Ok(resolved)
}

fn validate_declaration(name: &str, settings: &ServiceConfig) -> Result<()> {
    const BUILTINS: &[&str] = &[
        "postgres",
        "postgresql",
        "redis",
        "mongodb",
        "mysql",
        "docker",
        "headless_browser",
        "audio",
        "gpu",
        "video",
    ];
    if !vm_plugin::is_valid_plugin_name(name) || BUILTINS.contains(&name) {
        return Err(VmError::Config(format!(
            "Invalid service plugin key '{name}'"
        )));
    }
    let mut only_plugin = settings.clone();
    only_plugin.enabled = false;
    only_plugin.plugin = None;
    if serde_json::to_value(&only_plugin).map_err(|error| VmError::Config(error.to_string()))?
        != serde_json::to_value(ServiceConfig::default())
            .map_err(|error| VmError::Config(error.to_string()))?
    {
        return Err(VmError::Config(format!(
            "Service plugin '{name}' supports only enabled and plugin settings"
        )));
    }
    Ok(())
}

fn validate_dependencies(config: &VmConfig, plugins: &[ResolvedServicePlugin]) -> Result<()> {
    let names = plugins
        .iter()
        .map(|plugin| plugin.name.as_str())
        .collect::<HashSet<_>>();
    for plugin in plugins {
        for dependency in &plugin.depends_on {
            if dependency == "postgres"
                && config
                    .services
                    .get("postgresql")
                    .is_some_and(|service| service.enabled)
            {
                continue;
            }
            if !names.contains(dependency.as_str()) {
                return Err(VmError::Config(format!(
                    "Service plugin '{}' depends on inactive service '{dependency}'",
                    plugin.name
                )));
            }
        }
    }
    fn visit<'a>(
        name: &'a str,
        plugins: &'a [ResolvedServicePlugin],
        done: &mut HashSet<&'a str>,
        stack: &mut HashSet<&'a str>,
    ) -> Result<()> {
        if done.contains(name) {
            return Ok(());
        }
        if !stack.insert(name) {
            return Err(VmError::Config(format!(
                "Service plugin dependency cycle at '{name}'"
            )));
        }
        if let Some(plugin) = plugins.iter().find(|plugin| plugin.name == name) {
            for dependency in &plugin.depends_on {
                if dependency != "postgres" {
                    visit(dependency, plugins, done, stack)?;
                }
            }
        }
        stack.remove(name);
        done.insert(name);
        Ok(())
    }
    let mut done = HashSet::new();
    for plugin in plugins {
        visit(&plugin.name, plugins, &mut done, &mut HashSet::new())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use vm_plugin::{PluginInfo, PluginType};

    fn plugin(directory: &TempDir, name: &str, content: &str) -> Plugin {
        let path = directory.path().join(format!("{name}.yaml"));
        fs::write(&path, content).unwrap();
        Plugin {
            info: PluginInfo {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: None,
                author: None,
                plugin_type: PluginType::Service,
                preset_category: None,
            },
            content_file: path,
        }
    }

    fn config() -> VmConfig {
        serde_yaml_ng::from_str("provider: docker\nproject:\n  name: demo\nservices:\n  cache:\n    enabled: true\n    plugin: cache\n").unwrap()
    }

    #[test]
    fn resolves_only_activated_service_and_rejects_port_conflicts() {
        let directory = TempDir::new().unwrap();
        let cache = plugin(&directory, "cache", "image: redis:7-alpine\nports: ['6380:6379']\nvolumes: ['data:/data']\nenvironment:\n  MODE: safe\n");
        let mut config = config();
        let resolved = resolve_with(&config, |_| Ok(cache.clone())).unwrap();
        assert_eq!(resolved[0].ports[0].host, 6380);
        assert_eq!(resolved[0].volumes[0].name, "plugin_cache_data");
        config.ports.mappings.push(crate::ports::PortMapping {
            host: 6380,
            guest: 8080,
            protocol: Default::default(),
        });
        assert!(resolve_with(&config, |_| Ok(cache.clone()))
            .unwrap_err()
            .to_string()
            .contains("conflicts"));
        config.services.get_mut("cache").unwrap().enabled = false;
        assert!(
            resolve_with(&config, |_| panic!("inactive plugin must not load"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_unsupported_capabilities_and_provider_before_effects() {
        let directory = TempDir::new().unwrap();
        let scripted = plugin(
            &directory,
            "cache",
            "image: redis:7\ncommand: [sh, -c, echo unsafe]\n",
        );
        let config = config();
        assert!(resolve_with(&config, |_| Ok(scripted.clone()))
            .unwrap_err()
            .to_string()
            .contains("command"));
        let mut config = config;
        config.provider = Some("tart".into());
        assert!(resolve_with(&config, |_| panic!("provider check first"))
            .unwrap_err()
            .to_string()
            .contains("Docker or Podman"));
    }

    #[test]
    fn rejects_missing_dependency_and_cycles() {
        let directory = TempDir::new().unwrap();
        let first = plugin(
            &directory,
            "cache",
            "image: redis:7\ndepends_on: [metrics]\n",
        );
        let second = plugin(
            &directory,
            "metrics",
            "image: prom/prometheus:v2\ndepends_on: [cache]\n",
        );
        let mut config = config();
        assert!(resolve_with(&config, |_| Ok(first.clone()))
            .unwrap_err()
            .to_string()
            .contains("inactive"));
        config.services.insert(
            "metrics".into(),
            ServiceConfig {
                enabled: true,
                plugin: Some("metrics".into()),
                ..Default::default()
            },
        );
        assert!(resolve_with(&config, |name| Ok(if name == "cache" {
            first.clone()
        } else {
            second.clone()
        }))
        .unwrap_err()
        .to_string()
        .contains("cycle"));
    }
}
