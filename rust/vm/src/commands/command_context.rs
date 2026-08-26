//! Configuration, provider, and target assembly for command handlers.

use std::path::PathBuf;

use super::environment::resolve_environment;
use super::{packages, vm_ops};
use crate::cli::{Command, PackagesSubcommand};
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, AppConfig, GlobalConfig};
use vm_core::vm_progress;
use vm_provider::{get_provider, InstanceInfo, Provider};

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
            command: PackagesSubcommand::Status
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
        std::path::Path::new("/etc/vm/managed-guest").exists(),
        std::path::Path::new("/etc/vm/package-edge.env").exists()
            || std::path::Path::new(super::managed_guest::GUEST_REMOTE_COMMANDS_PATH).exists(),
    )
}

fn is_managed_guest(
    managed_marker: Option<&str>,
    compatible_image_identity: Option<&str>,
    docker_container: bool,
    canonical_filesystem_marker: bool,
    compatible_filesystem_marker: bool,
) -> bool {
    if canonical_filesystem_marker || managed_marker.is_some_and(truthy) {
        return true;
    }
    if compatible_filesystem_marker {
        tracing::warn!(
            compatibility = "legacy_managed_guest_file",
            "managed guest uses a retired filesystem marker; reconcile it before v6"
        );
        return true;
    }
    if docker_container
        && compatible_image_identity.is_some_and(|identity| !identity.trim().is_empty())
    {
        tracing::warn!(
            compatibility = "image_identity_managed_guest",
            "managed guest uses retired image-identity detection; reconcile it before v6"
        );
        return true;
    }
    false
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
    let explicit_config = config_path.is_some();
    let (mut provider, mut config, mut global_config) =
        load_provider_context(config_path, profile.clone(), provider_override)?;
    let instance =
        vm_ops::target::resolve_runtime_instance(provider.as_ref(), &config, requested_target)?;
    if requested_target.is_some() && !explicit_config {
        let target_config = provider.instance_config_path(&instance.name)?.ok_or_else(|| {
            VmError::validation(
                format!(
                    "Cannot locate the owning configuration for environment '{}'",
                    instance.name
                ),
                Some("Run the command from that project's directory or pass its vm.yaml with --config"),
            )
        })?;
        (provider, config, global_config) = load_provider_context(
            Some(target_config),
            profile,
            Some(instance.provider.clone()),
        )?;
    }
    Ok(RuntimeSubject {
        provider,
        config,
        global_config,
        target: instance.name,
    })
}

pub(super) fn load_runtime_subject_for_instance(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    instance: &InstanceInfo,
) -> VmResult<RuntimeSubject> {
    let target_config = target_config_path(config_path, instance, |instance| {
        let resolver = vm_ops::configured_provider(&VmConfig::default(), &instance.provider)?;
        resolver
            .instance_config_path(&instance.name)
            .map_err(Into::into)
    })?;
    let (provider, config, global_config) = load_provider_context(
        Some(target_config),
        profile,
        Some(instance.provider.clone()),
    )?;
    Ok(RuntimeSubject {
        provider,
        config,
        global_config,
        target: instance.name.clone(),
    })
}

fn target_config_path(
    explicit: Option<PathBuf>,
    instance: &InstanceInfo,
    ownership: impl FnOnce(&InstanceInfo) -> VmResult<Option<PathBuf>>,
) -> VmResult<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    ownership(instance)?.ok_or_else(|| {
        VmError::validation(
            format!(
                "Cannot locate the owning configuration for environment '{}'",
                instance.name
            ),
            Some("Pass its vm.yaml with --config"),
        )
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
    use super::{guest_allowed_command, host_command, is_managed_guest, target_config_path};
    use crate::cli::{Command, PackagesSubcommand};
    use std::path::PathBuf;
    use vm_provider::InstanceInfo;

    #[test]
    fn detects_canonical_and_compatibility_managed_guest_markers() {
        assert!(is_managed_guest(Some("1"), None, false, false, false));
        assert!(is_managed_guest(None, None, false, true, false));
        assert!(is_managed_guest(None, None, false, false, true));
        assert!(is_managed_guest(
            None,
            Some("demo:latest"),
            true,
            false,
            false
        ));
        assert!(!is_managed_guest(None, None, true, false, false));
    }

    #[test]
    fn guests_can_only_enter_agent_safe_package_commands() {
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Status,
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
                engine: crate::cli::PackageInfrastructureEngine::Auto,
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

    #[test]
    fn explicit_config_is_authoritative_for_an_inventory_target() {
        let instance = InstanceInfo {
            name: "other-dev".into(),
            id: "id".into(),
            status: "running".into(),
            provider: "docker".into(),
            project: Some("other".into()),
            uptime: None,
            created_at: None,
        };
        let explicit = PathBuf::from("/tmp/explicit/vm.yaml");
        let selected = target_config_path(Some(explicit.clone()), &instance, |_| {
            panic!("explicit config must bypass provider ownership lookup")
        })
        .unwrap();
        assert_eq!(selected, explicit);
    }
}
