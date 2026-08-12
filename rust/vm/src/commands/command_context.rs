//! Configuration, provider, and target assembly for command handlers.

use std::path::PathBuf;

use super::environment::resolve_environment;
use super::{packages, vm_ops};
use crate::cli::Command;
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, AppConfig, GlobalConfig};
use vm_core::vm_progress;
use vm_provider::{get_provider, Provider};

pub(super) fn ensure_controller_host(command: &Command) -> VmResult<()> {
    if !matches!(command, Command::Packages { .. } | Command::Tools { .. })
        || !is_managed_guest(
            std::env::var("VM_MANAGED_GUEST").ok().as_deref(),
            std::env::var("VM_IMAGE_IDENTITY").ok().as_deref(),
            std::path::Path::new("/.dockerenv").exists(),
            std::path::Path::new("/etc/vm/managed-guest").exists(),
        )
    {
        return Ok(());
    }

    let arguments = std::env::args_os()
        .skip(1)
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    Err(VmError::validation(
        "Package and managed-tool commands must run on the controller host",
        Some(format!("Run on the host: {}", host_command(&arguments))),
    ))
}

fn is_managed_guest(
    managed_marker: Option<&str>,
    image_identity: Option<&str>,
    docker_marker: bool,
    filesystem_marker: bool,
) -> bool {
    filesystem_marker
        || managed_marker.is_some_and(truthy)
        || (docker_marker && image_identity.is_some_and(|identity| !identity.trim().is_empty()))
}

fn truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn host_command(arguments: &[String]) -> String {
    std::iter::once("vm".to_string())
        .chain(arguments.iter().map(|argument| shell_quote(argument)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(argument: &str) -> String {
    if !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_@%+=:,./-".contains(character))
    {
        argument.to_string()
    } else {
        format!("'{}'", argument.replace('\'', "'\"'\"'"))
    }
}

pub(super) struct RuntimeSubject {
    pub(super) provider: Box<dyn Provider>,
    pub(super) config: VmConfig,
    pub(super) global_config: GlobalConfig,
    pub(super) target: String,
}

pub(super) fn load_provider_context(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    provider_override: Option<String>,
) -> VmResult<(Box<dyn Provider>, VmConfig, GlobalConfig)> {
    let app_config = AppConfig::load(config_path, profile, provider_override)?;
    let mut config = app_config.vm;
    packages::apply_client_environment(&mut config)?;
    let global_config = app_config.global;
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    Ok((provider, config, global_config))
}

pub(super) fn load_runtime_subject(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> VmResult<RuntimeSubject> {
    vm_progress!("Finding environment...");
    let resolved = resolve_environment(config_path.clone(), profile, environment)?;
    assemble_runtime_context(
        config_path,
        resolved.profile,
        resolved.provider_override,
        resolved.target.as_deref(),
    )
}

pub(super) async fn load_or_create_runtime_subject(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> VmResult<RuntimeSubject> {
    vm_progress!("Finding environment...");
    let resolved = resolve_environment(config_path.clone(), profile, environment)?;
    let (provider, config, global_config) =
        load_provider_context(config_path, resolved.profile, resolved.provider_override)?;
    let target = vm_ops::resolve_or_create_target(
        provider.as_ref(),
        &config,
        &global_config,
        resolved.target.as_deref(),
    )
    .await?;

    Ok(RuntimeSubject {
        provider,
        config,
        global_config,
        target,
    })
}

pub(super) fn load_runtime_context(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    provider_override: Option<String>,
    requested_target: Option<&str>,
) -> VmResult<RuntimeSubject> {
    vm_progress!("Finding environment...");
    assemble_runtime_context(config_path, profile, provider_override, requested_target)
}

fn assemble_runtime_context(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    provider_override: Option<String>,
    requested_target: Option<&str>,
) -> VmResult<RuntimeSubject> {
    let (provider, config, global_config) =
        load_provider_context(config_path, profile, provider_override)?;
    let target =
        vm_ops::target::resolve_runtime_target(provider.as_ref(), &config, requested_target)?;
    Ok(RuntimeSubject {
        provider,
        config,
        global_config,
        target,
    })
}

pub(super) fn project_name(config: &VmConfig) -> &str {
    config
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap_or("vm-project")
}

#[cfg(test)]
mod tests {
    use super::{host_command, is_managed_guest};

    #[test]
    fn detects_new_and_compatible_managed_guest_markers() {
        assert!(is_managed_guest(Some("1"), None, false, false));
        assert!(is_managed_guest(None, None, false, true));
        assert!(is_managed_guest(None, Some("demo:latest"), true, false));
        assert!(!is_managed_guest(None, None, true, false));
    }

    #[test]
    fn renders_the_exact_shell_safe_host_command() {
        let command = host_command(&[
            "tools".to_string(),
            "update".to_string(),
            "name with space".to_string(),
            "--all".to_string(),
        ]);

        assert_eq!(command, "vm tools update 'name with space' --all");
    }
}
