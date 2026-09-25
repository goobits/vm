use std::path::Path;

use serde_yaml_ng::Value;
use vm_core::error::{Result, VmError};

use crate::config::VmConfig;
use crate::config_ops::preset::resolve_declared_presets;
use crate::global_config::GlobalConfig;
use crate::validation::{validate_config, ValidationMode};

/// Validate the exact document that would be persisted before replacing the file.
pub(super) fn candidate(value: &Value, path: &Path, global: bool) -> Result<()> {
    if global {
        let config: GlobalConfig = serde_yaml_ng::from_value(value.clone())?;
        let tools = crate::config::ToolsConfig {
            entries: config.tools,
            ..Default::default()
        };
        tools.validate()?;
        return Ok(());
    }

    let config: VmConfig = serde_yaml_ng::from_value(value.clone())?;
    let project_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let resolved = resolve_declared_presets(config, project_dir)?;
    if resolved.environments.is_empty() || resolved.provider.is_some() {
        validate_static(&resolved, "project")?;
    }
    if let Some(default) = resolved
        .project
        .as_ref()
        .and_then(|project| project.default_environment.as_deref())
    {
        if !resolved.environments.contains_key(default) {
            return Err(VmError::Config(format!(
                "Default environment '{default}' is not declared in environments"
            )));
        }
    }
    let mut base = resolved.clone();
    base.environments.clear();
    for (name, declaration) in &resolved.environments {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(VmError::Config(format!(
                "Invalid environment name '{name}': use only alphanumeric characters, dashes, and underscores"
            )));
        }
        validate_static(
            &declaration.apply_to(&base),
            &format!("environment '{name}'"),
        )?;
    }
    Ok(())
}

fn validate_static(config: &VmConfig, label: &str) -> Result<()> {
    let report = validate_config(config, ValidationMode::Static)
        .map_err(|error| VmError::Config(format!("{label} validation failed: {error}")))?;
    if report.has_errors() {
        return Err(VmError::Config(format!("{label} is invalid:\n{report}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::candidate;

    #[test]
    fn validates_declared_environment_and_default_before_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let valid: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            "project:\n  name: test\n  default_environment: dev\nenvironments:\n  dev:\n    provider: docker\n    image: ubuntu:24.04\n",
        )
        .unwrap();
        candidate(&valid, &path, false).unwrap();

        let missing_default: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            "project:\n  name: test\n  default_environment: prod\nenvironments:\n  dev:\n    provider: docker\n    image: ubuntu:24.04\n",
        )
        .unwrap();
        assert!(candidate(&missing_default, &path, false)
            .unwrap_err()
            .to_string()
            .contains("Default environment 'prod'"));
    }

    #[test]
    fn rejects_invalid_declared_environment_without_touching_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        let invalid: serde_yaml_ng::Value = serde_yaml_ng::from_str(
            "project:\n  name: test\nenvironments:\n  bad/name:\n    provider: docker\n    image: ubuntu:24.04\n",
        )
        .unwrap();
        assert!(candidate(&invalid, &path, false)
            .unwrap_err()
            .to_string()
            .contains("Invalid environment name"));
        assert!(!path.exists());
    }
}
