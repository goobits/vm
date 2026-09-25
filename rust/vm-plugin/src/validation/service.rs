use anyhow::Result;
use std::collections::HashSet;

use crate::types::{Plugin, ServiceContent};

use super::{ValidationError, ValidationResult};

pub(super) fn validate(plugin: &Plugin, result: &mut ValidationResult) -> Result<()> {
    let content = match crate::discovery::load_service_content(plugin) {
        Ok(content) => content,
        Err(error) => {
            result.add_error(ValidationError::new("service_content", error.to_string()));
            return Ok(());
        }
    };
    validate_content(&content, result);
    Ok(())
}

fn validate_content(content: &ServiceContent, result: &mut ValidationResult) {
    if content.image.trim().is_empty()
        || content.image.chars().any(char::is_whitespace)
        || content.image.contains('$')
    {
        result.add_error(ValidationError::new(
            "image",
            "Specify one container image reference",
        ));
    }
    if !content.image.contains(':') {
        result.add_warning("Pin the service image to a specific tag or digest".to_string());
    }
    if content.command.is_some() {
        result.add_error(ValidationError::new(
            "command",
            "Command overrides are unsupported for service plugins",
        ));
    }
    if content.health_check.is_some() {
        result.add_error(ValidationError::new(
            "health_check",
            "Executable health checks are unsupported for service plugins",
        ));
    }
    let mut ports = HashSet::new();
    for mapping in &content.ports {
        validate_port_mapping(mapping, result);
        if !ports.insert(mapping.split(':').next().unwrap_or_default()) {
            result.add_error(ValidationError::new(
                "ports",
                format!("Duplicate host port in '{mapping}'"),
            ));
        }
    }
    let mut targets = HashSet::new();
    for volume in &content.volumes {
        let Some((source, target)) = volume.split_once(':') else {
            result.add_error(ValidationError::new(
                "volumes",
                format!("Invalid named volume mapping '{volume}'"),
            ));
            continue;
        };
        if volume.matches(':').count() != 1
            || !is_safe_name(source)
            || !target.starts_with('/')
            || target == "/"
            || target.contains('$')
            || target
                .split('/')
                .any(|component| component == ".." || component == ".")
        {
            result.add_error(ValidationError::new(
                "volumes",
                format!("Only named volumes with absolute targets are supported: '{volume}'"),
            ));
        }
        if !targets.insert(target) {
            result.add_error(ValidationError::new(
                "volumes",
                format!("Duplicate volume target '{target}'"),
            ));
        }
    }
    let mut dependencies = HashSet::new();
    for dependency in &content.depends_on {
        if !is_safe_name(dependency) || !dependencies.insert(dependency) {
            result.add_error(ValidationError::new(
                "depends_on",
                format!("Invalid or duplicate dependency '{dependency}'"),
            ));
        }
    }
    super::validate_environment(&content.environment, result);
    for (key, value) in &content.environment {
        if !key
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
            || !key
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            result.add_error(ValidationError::new(
                "environment",
                format!("Invalid container environment name '{key}'"),
            ));
        }
        if value.contains('$') {
            result.add_error(ValidationError::new(
                "environment",
                format!("Environment value '{key}' cannot contain Compose interpolation ('$')"),
            ));
        }
    }
}

fn is_safe_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
}

pub(super) fn validate_port_mapping(port: &str, result: &mut ValidationResult) {
    let parts = port.split(':').collect::<Vec<_>>();
    if !matches!(parts.len(), 1 | 2)
        || parts
            .iter()
            .any(|part| part.parse::<u16>().map_or(true, |number| number == 0))
    {
        result.add_error(ValidationError::new(
            "ports",
            format!("Invalid port mapping '{port}'; use port or host:container, 1-65535"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_manifest_accepts_only_declarative_container_fields() {
        let content: ServiceContent = serde_yaml_ng::from_str(
            "image: redis:7-alpine\nports: ['6380:6379']\nvolumes: ['data:/data']\n",
        )
        .unwrap();
        let mut result = ValidationResult::new();
        validate_content(&content, &mut result);
        assert!(result.is_valid, "{:?}", result.errors);

        for manifest in [
            "image: redis:7\ncommand: [sh, -c, echo hi]\n",
            "image: redis:7\nhealth_check: 'curl localhost'\n",
            "image: redis:7\nvolumes: ['/host:/data']\n",
            "image: redis:7\nports: ['0:6379']\n",
            "image: redis:7\nenvironment:\n  TOKEN: '${HOST_TOKEN}'\n",
        ] {
            let content: ServiceContent = serde_yaml_ng::from_str(manifest).unwrap();
            let mut result = ValidationResult::new();
            validate_content(&content, &mut result);
            assert!(!result.is_valid, "{manifest}");
        }
    }
}
