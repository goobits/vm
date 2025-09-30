use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Plugin manifest file (plugin.yaml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub author: Option<String>,

    #[serde(default)]
    pub presets: Vec<PluginPreset>,

    #[serde(default)]
    pub services: Vec<PluginService>,
}

/// Plugin preset definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPreset {
    pub name: String,
    pub description: Option<String>,
    pub base_image: String,

    #[serde(default)]
    pub packages: Vec<String>,

    #[serde(default)]
    pub services: Vec<String>,

    #[serde(default)]
    pub env: HashMap<String, String>,

    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub volumes: Vec<String>,

    #[serde(default)]
    pub provision: Vec<String>,
}

/// Plugin service definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginService {
    pub name: String,
    pub description: Option<String>,
    pub image: String,

    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub volumes: Vec<String>,

    #[serde(default)]
    pub env: HashMap<String, String>,

    #[serde(default)]
    pub command: Option<Vec<String>>,

    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_minimal_manifest() {
        let yaml = r#"
name: test-plugin
version: 1.0.0
"#;
        let manifest: PluginManifest = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.presets.is_empty());
        assert!(manifest.services.is_empty());
    }

    #[test]
    fn test_deserialize_full_manifest() {
        let yaml = r#"
name: test-plugin
version: 1.0.0
description: A test plugin
author: Test Author
presets:
  - name: test-preset
    description: Test preset
    base_image: ubuntu:22.04
    packages:
      - curl
      - git
    services:
      - postgres
    env:
      FOO: bar
    ports:
      - "8080:8080"
    volumes:
      - "./data:/data"
    provision:
      - echo "Hello"
services:
  - name: test-service
    description: Test service
    image: postgres:15
    ports:
      - "5432:5432"
    volumes:
      - "pgdata:/var/lib/postgresql/data"
    env:
      POSTGRES_PASSWORD: secret
    depends_on:
      - redis
"#;
        let manifest: PluginManifest = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.description, Some("A test plugin".to_string()));
        assert_eq!(manifest.author, Some("Test Author".to_string()));
        assert_eq!(manifest.presets.len(), 1);
        assert_eq!(manifest.services.len(), 1);

        let preset = &manifest.presets[0];
        assert_eq!(preset.name, "test-preset");
        assert_eq!(preset.base_image, "ubuntu:22.04");
        assert_eq!(preset.packages.len(), 2);
        assert_eq!(preset.services.len(), 1);
        assert_eq!(preset.env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(preset.ports.len(), 1);
        assert_eq!(preset.volumes.len(), 1);
        assert_eq!(preset.provision.len(), 1);

        let service = &manifest.services[0];
        assert_eq!(service.name, "test-service");
        assert_eq!(service.image, "postgres:15");
        assert_eq!(service.ports.len(), 1);
        assert_eq!(service.volumes.len(), 1);
        assert_eq!(
            service.env.get("POSTGRES_PASSWORD"),
            Some(&"secret".to_string())
        );
        assert_eq!(service.depends_on.len(), 1);
    }
}
