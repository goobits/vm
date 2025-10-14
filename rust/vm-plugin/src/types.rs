use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Plugin metadata (stored in plugin.yaml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub plugin_type: PluginType,
}

/// Plugin type discriminator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Preset,
    Service,
}

/// Complete plugin with metadata and content file path
#[derive(Debug, Clone)]
pub struct Plugin {
    pub info: PluginInfo,
    pub content_file: PathBuf,
}

/// Preset content (stored in preset.yaml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetContent {
    #[serde(default)]
    pub packages: Vec<String>,

    #[serde(default)]
    pub npm_packages: Vec<String>,

    #[serde(default)]
    pub pip_packages: Vec<String>,

    #[serde(default)]
    pub cargo_packages: Vec<String>,

    #[serde(default)]
    pub services: Vec<String>,

    #[serde(default)]
    pub environment: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub aliases: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub provision: Vec<String>,
}

/// Service content (stored in service.yaml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceContent {
    pub image: String,

    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub volumes: Vec<String>,

    #[serde(default)]
    pub environment: std::collections::HashMap<String, String>,

    #[serde(default)]
    pub command: Option<Vec<String>>,

    #[serde(default)]
    pub depends_on: Vec<String>,

    #[serde(default)]
    pub health_check: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_plugin_info_preset() {
        let yaml = r#"
name: rust-advanced
version: 1.0.0
description: Advanced Rust development environment
author: Example Author
plugin_type: preset
"#;
        let info: PluginInfo =
            serde_yaml_ng::from_str(yaml).expect("should deserialize preset info");
        assert_eq!(info.name, "rust-advanced");
        assert_eq!(info.version, "1.0.0");
        assert_eq!(info.plugin_type, PluginType::Preset);
    }

    #[test]
    fn test_deserialize_plugin_info_service() {
        let yaml = r#"
name: redis-sentinel
version: 2.0.0
plugin_type: service
"#;
        let info: PluginInfo =
            serde_yaml_ng::from_str(yaml).expect("should deserialize service info");
        assert_eq!(info.name, "redis-sentinel");
        assert_eq!(info.plugin_type, PluginType::Service);
    }

    #[test]
    fn test_deserialize_preset_content() {
        let yaml = r#"
packages:
  - curl
  - git
npm_packages:
  - typescript
services:
  - postgres
environment:
  RUST_LOG: debug
provision:
  - echo "Setup complete"
"#;
        let content: PresetContent =
            serde_yaml_ng::from_str(yaml).expect("should deserialize preset content");
        assert_eq!(content.packages.len(), 2);
        assert_eq!(content.npm_packages.len(), 1);
        assert_eq!(content.services.len(), 1);
        assert_eq!(
            content.environment.get("RUST_LOG"),
            Some(&"debug".to_string())
        );
        assert_eq!(content.provision.len(), 1);
    }

    #[test]
    fn test_deserialize_service_content() {
        let yaml = r#"
image: redis:7-alpine
ports:
  - "6379:6379"
volumes:
  - "redis_data:/data"
environment:
  REDIS_PASSWORD: secret
depends_on:
  - postgres
"#;
        let content: ServiceContent =
            serde_yaml_ng::from_str(yaml).expect("should deserialize service content");
        assert_eq!(content.image, "redis:7-alpine");
        assert_eq!(content.ports.len(), 1);
        assert_eq!(content.volumes.len(), 1);
        assert_eq!(
            content.environment.get("REDIS_PASSWORD"),
            Some(&"secret".to_string())
        );
        assert_eq!(content.depends_on.len(), 1);
    }

    #[test]
    fn test_plugin_type_serialization() {
        let preset = PluginType::Preset;
        let service = PluginType::Service;

        let preset_yaml =
            serde_yaml_ng::to_string(&preset).expect("should serialize preset type");
        let service_yaml =
            serde_yaml_ng::to_string(&service).expect("should serialize service type");

        assert!(preset_yaml.contains("preset"));
        assert!(service_yaml.contains("service"));
    }
}
