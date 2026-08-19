//! Configuration, provider, and target assembly for command handlers.

use std::path::PathBuf;

use super::environment::resolve_environment;
use super::{packages, vm_ops};
use crate::cli::{Command, PackagesSubcommand};
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, AppConfig, GlobalConfig};
use vm_core::vm_progress;
use vm_provider::{get_provider, Provider};

pub(super) fn ensure_controller_host(command: &Command) -> VmResult<()> {
    if !managed_guest_context()
        || !matches!(command, Command::Packages { .. } | Command::Tools { .. })
    {
        return Ok(());
    }
    if guest_allowed_command(command) {
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

fn guest_allowed_command(command: &Command) -> bool {
    matches!(
        command,
        Command::Packages {
            command: PackagesSubcommand::Status { .. }
                | PackagesSubcommand::Checkout { .. }
                | PackagesSubcommand::Show { .. }
                | PackagesSubcommand::Release
                | PackagesSubcommand::Cancel
        }
    )
}

pub(super) fn managed_guest_context() -> bool {
    if std::env::var("VM_TEST_MODE").is_ok() {
        match std::env::var("VM_TEST_COMMAND_CONTEXT").ok().as_deref() {
            Some("host") => return false,
            Some("guest") => return true,
            _ => {}
        }
    }
    is_managed_guest(
        std::env::var("VM_MANAGED_GUEST").ok().as_deref(),
        std::env::var("VM_IMAGE_IDENTITY").ok().as_deref(),
        std::path::Path::new("/.dockerenv").exists(),
        std::path::Path::new("/etc/vm/managed-guest").exists()
            || std::path::Path::new("/etc/vm/package-edge.env").exists()
            || std::path::Path::new(super::managed_guest::GUEST_REMOTE_COMMANDS_PATH).exists(),
    )
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

pub(super) fn host_command(arguments: &[String]) -> String {
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
    let (mut provider, mut config, mut global_config) =
        load_provider_context(config_path, profile, provider_override)?;
    let target =
        vm_ops::target::resolve_runtime_target(provider.as_ref(), &config, requested_target)?;
    if requested_target.is_some()
        && target_belongs_to_another_project(provider.as_ref(), &target, &config)
    {
        let target_config = provider.instance_config_path(&target)?.ok_or_else(|| {
            VmError::validation(
                format!("Cannot locate the owning configuration for environment '{target}'"),
                Some("Run the command from that project's directory or pass its vm.yaml with --config"),
            )
        })?;
        let app_config = AppConfig::load(Some(target_config), None, None)?;
        config = app_config.vm;
        packages::apply_client_environment(&mut config)?;
        global_config = app_config.global;
        provider = get_provider(config.clone()).map_err(VmError::from)?;
    }
    Ok(RuntimeSubject {
        provider,
        config,
        global_config,
        target,
    })
}

fn target_belongs_to_another_project(
    provider: &dyn Provider,
    target: &str,
    config: &VmConfig,
) -> bool {
    let current = project_name(config);
    provider.list_instances().ok().is_some_and(|instances| {
        instances.into_iter().any(|instance| {
            instance.name == target
                && instance
                    .project
                    .as_deref()
                    .is_some_and(|project| project != current)
        })
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
    use super::{guest_allowed_command, host_command, is_managed_guest};
    use crate::cli::{Command, PackagesSubcommand};

    #[test]
    fn detects_new_and_compatible_managed_guest_markers() {
        assert!(is_managed_guest(Some("1"), None, false, false));
        assert!(is_managed_guest(None, None, false, true));
        assert!(is_managed_guest(None, Some("demo:latest"), true, false));
        assert!(!is_managed_guest(None, None, true, false));
    }

    #[test]
    fn guests_can_only_enter_agent_safe_package_commands() {
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Status {
                runtime: crate::cli::PackageInfrastructureRuntime::Auto,
            },
        }));
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Show {
                checkout_id: "checkout-1".into(),
            },
        }));
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Release,
        }));
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Cancel,
        }));
        assert!(!guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Up {
                runtime: crate::cli::PackageInfrastructureRuntime::Auto,
                port: 3080,
                registry_image: None,
                job_image: None,
            },
        }));
    }

    #[test]
    fn renders_the_exact_shell_safe_host_command() {
        let command = host_command(&[
            "tools".to_string(),
            "update".to_string(),
            "name with space".to_string(),
        ]);

        assert_eq!(command, "vm tools update 'name with space'");
    }
}
