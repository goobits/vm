//! Configuration, provider, and target assembly for command handlers.

use std::path::PathBuf;

use super::environment::resolve_environment;
use super::{packages, vm_ops};
use crate::cli::{Command, PackageServiceSubcommand, PackagesSubcommand};
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
            command: PackagesSubcommand::Service {
                command: PackageServiceSubcommand::Status
            } | PackagesSubcommand::Checkout { .. }
                | PackagesSubcommand::CheckoutShow { .. }
                | PackagesSubcommand::Release { .. }
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
        std::path::Path::new("/etc/vm/managed-guest").exists(),
    )
}

fn is_managed_guest(managed_marker: Option<&str>, canonical_filesystem_marker: bool) -> bool {
    canonical_filesystem_marker || managed_marker.is_some_and(truthy)
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

pub(super) struct PreparedStart {
    pub(super) subject: RuntimeSubject,
    pub(super) create_name: Option<String>,
}

struct EnvironmentContext {
    provider: Box<dyn Provider>,
    config: VmConfig,
    global_config: GlobalConfig,
    selected: Option<String>,
}

pub(super) fn prepare_start(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
) -> VmResult<PreparedStart> {
    let resolved = resolve_environment(config_path.clone(), profile, environment)?;
    let EnvironmentContext {
        provider,
        config,
        global_config,
        selected,
    } = load_environment_provider_context(
        config_path,
        resolved.profile,
        resolved.provider_override,
        resolved.target.as_deref(),
    )?;
    let existing =
        vm_ops::target::find_runtime_target(provider.as_ref(), &config, selected.as_deref())?;
    let (target, create_name) = match existing {
        Some(instance) => (instance.name, None),
        None => {
            let name = selected.ok_or_else(|| {
                VmError::validation(
                    "No declared default environment exists",
                    Some("Declare one with `vm create NAME --provider ... --image ...`"),
                )
            })?;
            if !config.environments.contains_key(&name) {
                return Err(VmError::validation(
                    format!("Environment '{name}' is not declared"),
                    Some("Declare it with `vm create NAME --provider ... --image ...`"),
                ));
            }
            let project = project_name(&config);
            let target =
                vm_ops::target::canonical_instance_name(provider.name(), project, Some(&name));
            (target, Some(name))
        }
    };
    Ok(PreparedStart {
        subject: configure_runtime_subject(config, global_config, target)?,
        create_name,
    })
}

pub(super) fn load_provider_context(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    provider_override: Option<String>,
) -> VmResult<(Box<dyn Provider>, VmConfig, GlobalConfig)> {
    let app_config = AppConfig::load(config_path, profile, provider_override)?;
    let config = app_config.vm;
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
    let EnvironmentContext {
        provider,
        config,
        global_config,
        selected,
    } = load_environment_provider_context(
        config_path,
        profile,
        provider_override,
        requested_target,
    )?;
    let instance =
        vm_ops::target::resolve_runtime_instance(provider.as_ref(), &config, selected.as_deref())?;
    configure_runtime_subject(config, global_config, instance.name)
}

fn load_environment_provider_context(
    config_path: Option<PathBuf>,
    profile: Option<String>,
    provider_override: Option<String>,
    requested_target: Option<&str>,
) -> VmResult<EnvironmentContext> {
    let app = AppConfig::load(config_path, profile, provider_override)?;
    require_project_config(&app.vm)?;
    let selected = super::declarations::selected_name(&app.vm, requested_target)?;
    let config = selected
        .as_deref()
        .and_then(|name| app.vm.environments.get(name))
        .map_or_else(
            || app.vm.clone(),
            |declaration| declaration.apply_to(&app.vm),
        );
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    Ok(EnvironmentContext {
        provider,
        config,
        global_config: app.global,
        selected,
    })
}

pub(super) fn require_project_config(config: &VmConfig) -> VmResult<()> {
    if config.owning_config_path().is_none()
        || config
            .project
            .as_ref()
            .and_then(|project| project.name.as_deref())
            .is_none()
    {
        return Err(VmError::validation(
            "An environment operation requires a project configuration",
            Some("Run from a project containing vm.yaml or select one with --project"),
        ));
    }
    Ok(())
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
    let app = AppConfig::load(
        Some(target_config),
        profile,
        Some(instance.provider.clone()),
    )?;
    let config = config_for_named_instance(&app.vm, instance);
    configure_runtime_subject(config, app.global, instance.name.clone())
}

fn configure_runtime_subject(
    mut config: VmConfig,
    global_config: GlobalConfig,
    target: String,
) -> VmResult<RuntimeSubject> {
    packages::apply_client_environment(&mut config, &target)?;
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    Ok(RuntimeSubject {
        provider,
        config,
        global_config,
        target,
    })
}

fn config_for_named_instance(config: &VmConfig, instance: &InstanceInfo) -> VmConfig {
    let project = project_name(config);
    config
        .environments
        .iter()
        .find(|(name, declaration)| {
            declaration.provider.as_str() == instance.provider
                && vm_ops::target::canonical_instance_name(&instance.provider, project, Some(name))
                    == instance.name
        })
        .map_or_else(
            || config.clone(),
            |(_, declaration)| declaration.apply_to(config),
        )
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
    use super::{
        config_for_named_instance, guest_allowed_command, host_command, is_managed_guest,
        target_config_path,
    };
    use crate::cli::{Command, PackageServiceSubcommand, PackagesSubcommand};
    use std::path::PathBuf;
    use vm_provider::InstanceInfo;

    #[test]
    fn fleet_subject_uses_the_named_environment_declaration() {
        let config: vm_config::config::VmConfig = serde_yaml_ng::from_str(
            r#"
project:
  name: demo
provider: docker
environments:
  test:
    provider: podman
    image: postgres:17
    services:
      postgresql:
        enabled: true
        user: test_admin
"#,
        )
        .unwrap();
        let instance = InstanceInfo {
            name: "demo-test-dev".into(),
            id: "instance".into(),
            status: "running".into(),
            provider: "podman".into(),
            project: Some("demo".into()),
            uptime: None,
            created_at: None,
        };
        let selected = config_for_named_instance(&config, &instance);
        assert_eq!(selected.provider.unwrap().as_str(), "podman");
        assert_eq!(
            selected.services["postgresql"].user.as_deref(),
            Some("test_admin")
        );

        let unrelated = InstanceInfo {
            name: "demo-other-dev".into(),
            ..instance
        };
        assert!(config_for_named_instance(&config, &unrelated)
            .services
            .is_empty());
    }

    #[test]
    fn detects_only_canonical_managed_guest_markers() {
        assert!(is_managed_guest(Some("1"), false));
        assert!(is_managed_guest(None, true));
        assert!(!is_managed_guest(None, false));
    }

    #[test]
    fn guests_can_only_enter_agent_safe_package_commands() {
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Service {
                command: PackageServiceSubcommand::Status
            },
        }));
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::CheckoutShow {
                checkout_id: "checkout-1".into(),
            },
        }));
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Release {
                receipt: None,
                background: false,
            },
        }));
        assert!(guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Cancel,
        }));
        assert!(!guest_allowed_command(&Command::Packages {
            command: PackagesSubcommand::Up {
                engine: crate::cli::PackageInfrastructureEngine::Auto,
                port: None,
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
