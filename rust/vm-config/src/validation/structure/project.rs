use crate::config::{mounts::validate_mount_target, BoxSpec, ProviderName, VmConfig};
use vm_core::error::Result;

/// Validate box spec configurations are compatible with the provider
fn validate_box_spec(config: &VmConfig, provider: &ProviderName) -> Vec<String> {
    let mut errors = Vec::new();

    let Some(vm) = &config.vm else {
        return errors;
    };
    let Some(box_spec) = vm.get_box_spec() else {
        return errors;
    };

    match provider {
        ProviderName::Docker | ProviderName::Podman => {
            validate_docker_box_spec(&box_spec, &mut errors)
        }
        ProviderName::Tart => validate_tart_box_spec(&box_spec, &mut errors),
        _ => {}
    }

    errors
}

fn validate_docker_box_spec(box_spec: &BoxSpec, errors: &mut Vec<String>) {
    if let BoxSpec::Build { dockerfile, .. } = box_spec {
        let path = std::path::Path::new(dockerfile);
        if !path.exists() {
            errors.push(format!("Dockerfile not found: {}", dockerfile));
        }
    }
}

fn validate_tart_box_spec(box_spec: &BoxSpec, errors: &mut Vec<String>) {
    if matches!(box_spec, BoxSpec::Build { .. }) {
        errors.push("Tart does not support Dockerfile builds".to_string());
    }
}

pub(super) fn validate_required_fields(config: &VmConfig) -> Result<()> {
    if config.provider.is_none() {
        return Err(vm_core::error::VmError::Config(
            "Missing required field: provider".to_string(),
        ));
    }

    if let Some(project) = &config.project {
        if project.name.is_none() {
            return Err(vm_core::error::VmError::Config(
                "Missing required field: project.name".to_string(),
            ));
        }
    } else {
        return Err(vm_core::error::VmError::Config(
            "Missing required field: project".to_string(),
        ));
    }

    Ok(())
}

pub(super) fn validate_provider(config: &VmConfig) -> Result<()> {
    if let Some(provider) = &config.provider {
        if provider.is_supported() {
            Ok(())
        } else {
            Err(vm_core::error::VmError::Config(format!(
                "Invalid provider '{}'. Valid providers are: {}",
                provider,
                ProviderName::SUPPORTED.join(", ")
            )))
        }
    } else {
        Ok(())
    }
}

pub(super) fn validate_box_spec_compat(config: &VmConfig) -> Result<()> {
    if let Some(provider) = &config.provider {
        let errors = validate_box_spec(config, provider);
        if !errors.is_empty() {
            return Err(vm_core::error::VmError::Config(errors.join("; ")));
        }
    }
    Ok(())
}

pub(super) fn validate_project(config: &VmConfig) -> Result<()> {
    if let Some(project) = &config.project {
        if let Some(name) = &project.name {
            let is_valid = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !is_valid {
                return Err(vm_core::error::VmError::Config(format!(
                    "Invalid project name '{name}': use only alphanumeric characters, dashes, and underscores"
                )));
            }
        }

        if let Some(hostname) = &project.hostname {
            let is_valid = !hostname.is_empty()
                && hostname
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
            if !is_valid {
                return Err(vm_core::error::VmError::Config(format!(
                    "Invalid hostname '{hostname}'"
                )));
            }
        }

        if let Some(path) = &project.workspace_path {
            validate_mount_target(std::path::Path::new(path))?;
        }
    }

    Ok(())
}
