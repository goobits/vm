//! Stable plugin read views that exclude executable and secret-bearing fields.

use anyhow::{anyhow, Result};
use serde::Serialize;
use vm_plugin::{load_preset_content, load_service_content, Plugin, PluginType, PresetCategory};

#[derive(Serialize)]
pub(super) struct PluginListView<'a> {
    pub plugins: Vec<PluginMetadata<'a>>,
}

#[derive(Serialize)]
pub(super) struct PluginMetadata<'a> {
    name: &'a str,
    version: &'a str,
    #[serde(rename = "type")]
    plugin_type: PluginType,
    description: Option<&'a str>,
    author: Option<&'a str>,
    preset_category: Option<&'a PresetCategory>,
}

#[derive(Serialize)]
pub(super) struct PluginShowView<'a> {
    pub plugin: PluginMetadata<'a>,
    pub details: PluginDetails,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(super) enum PluginDetails {
    Preset {
        packages: Vec<String>,
        npm_packages: Vec<String>,
        pip_packages: Vec<String>,
        cargo_packages: Vec<String>,
        services: Vec<String>,
        environment_variable_count: usize,
        alias_count: usize,
        provision_step_count: usize,
    },
    Service {
        image: String,
        ports: Vec<String>,
        volume_count: usize,
        environment_variable_count: usize,
        has_command: bool,
        dependencies: Vec<String>,
        has_health_check: bool,
    },
}

pub(super) fn list(plugins: &[Plugin]) -> PluginListView<'_> {
    let mut plugins = plugins.iter().map(metadata).collect::<Vec<_>>();
    plugins.sort_by(|a, b| (a.name, kind(a.plugin_type)).cmp(&(b.name, kind(b.plugin_type))));
    PluginListView { plugins }
}

pub(super) fn show(plugin: &Plugin) -> Result<PluginShowView<'_>> {
    let details = match plugin.info.plugin_type {
        PluginType::Preset => {
            let content = load_preset_content(plugin)
                .map_err(|_| anyhow!("Plugin '{}' content could not be read", plugin.info.name))?;
            PluginDetails::Preset {
                packages: content.packages,
                npm_packages: content.npm_packages,
                pip_packages: content.pip_packages,
                cargo_packages: content.cargo_packages,
                services: content.services,
                environment_variable_count: content.environment.len(),
                alias_count: content.aliases.len(),
                provision_step_count: content.provision.len(),
            }
        }
        PluginType::Service => {
            let content = load_service_content(plugin)
                .map_err(|_| anyhow!("Plugin '{}' content could not be read", plugin.info.name))?;
            PluginDetails::Service {
                image: content.image,
                ports: content.ports,
                volume_count: content.volumes.len(),
                environment_variable_count: content.environment.len(),
                has_command: content.command.is_some(),
                dependencies: content.depends_on,
                has_health_check: content.health_check.is_some(),
            }
        }
    };
    Ok(PluginShowView {
        plugin: metadata(plugin),
        details,
    })
}

fn metadata(plugin: &Plugin) -> PluginMetadata<'_> {
    PluginMetadata {
        name: &plugin.info.name,
        version: &plugin.info.version,
        plugin_type: plugin.info.plugin_type,
        description: plugin.info.description.as_deref(),
        author: plugin.info.author.as_deref(),
        preset_category: plugin.info.preset_category.as_ref(),
    }
}

fn kind(plugin_type: PluginType) -> &'static str {
    match plugin_type {
        PluginType::Preset => "preset",
        PluginType::Service => "service",
    }
}

#[cfg(test)]
mod tests {
    use super::{list, show};
    use vm_plugin::{Plugin, PluginInfo, PluginType};

    fn plugin(path: std::path::PathBuf, kind: PluginType) -> Plugin {
        Plugin {
            info: PluginInfo {
                name: "demo".into(),
                version: "1.0.0".into(),
                description: None,
                author: Some("Example".into()),
                plugin_type: kind,
                preset_category: None,
            },
            content_file: path,
        }
    }

    #[test]
    fn list_is_typed_and_excludes_installed_paths() {
        let plugin = plugin(
            "/private/plugins/demo/service.yaml".into(),
            PluginType::Service,
        );
        let value = serde_json::to_value(list(&[plugin])).unwrap();
        assert_eq!(value["plugins"][0]["type"], "service");
        assert_eq!(value["plugins"][0]["name"], "demo");
        assert!(!value.to_string().contains("/private/plugins"));
    }

    #[test]
    fn show_redacts_environment_values_commands_and_mounts() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("service.yaml");
        std::fs::write(&file, "image: example/api:1\nports: ['8080:80']\nvolumes: ['/private/data:/data']\nenvironment: {TOKEN: supersecret}\ncommand: ['--password=secret']\n").unwrap();
        let value =
            serde_json::to_value(show(&plugin(file, PluginType::Service)).unwrap()).unwrap();
        assert_eq!(value["details"]["kind"], "service");
        assert_eq!(value["details"]["image"], "example/api:1");
        assert_eq!(value["details"]["volume_count"], 1);
        assert_eq!(value["details"]["environment_variable_count"], 1);
        for secret in [
            "supersecret",
            "--password=secret",
            "/private/data",
            directory.path().to_str().unwrap(),
        ] {
            assert!(!value.to_string().contains(secret));
        }
    }
}
