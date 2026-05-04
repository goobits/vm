// Command handlers for VM operations

use crate::cli::{
    Args, Command, EnvironmentKind, PluginSubcommand, SecretSubcommand, SystemSubcommand,
    TunnelSubcommand,
};
use crate::error::{VmError, VmResult};
use environment::{mac_profile, resolve_environment};
use vm_config::{
    config::{BoxSpec, CpuLimit, MemoryLimit, TartConfig, VmConfig},
    AppConfig,
};
use vm_core::{vm_error, vm_println};
use vm_messages::messages::MESSAGES;
use vm_provider::get_provider;

const ZSH_COMPLETION_PRELUDE: &str = r#"# Ensure compdef is available when this file is sourced directly from .zshrc.
if [[ -n ${ZSH_VERSION:-} && -z ${functions[compdef]+x} ]]; then
  autoload -Uz compinit
  compinit -i
fi

"#;

pub mod base;
pub mod clean;
pub mod config;
pub mod db;
pub mod doctor;
mod environment;
pub mod init;
pub mod plugin;
pub mod plugin_new;
pub mod registry;
pub mod secrets;
pub mod tunnel;
pub mod uninstall;
pub mod update;
pub mod vm_ops;

#[must_use = "command execution results should be handled"]
pub async fn execute_command(args: Args) -> VmResult<()> {
    if args.dry_run {
        print_dry_run_summary(&args);
        return Ok(());
    }

    match args.command {
        Command::Doctor { fix, clean } => {
            if clean {
                clean::handle_clean(false, false).await?;
            }
            doctor::run_with_fix(fix).map_err(VmError::from)
        }
        Command::Config { command } => config::handle_config_command(&command, false, args.profile),
        Command::Plugin { command } => handle_plugin_command(&command),
        Command::Db { command } => db::handle_db(command).await,
        Command::Fleet { command } => vm_ops::handle_fleet_command(&command, false).await,
        Command::Secret { command } => {
            handle_secret_command(&command, args.config, args.profile).await
        }
        Command::System { command } => {
            handle_system_command(&command, args.config, args.profile).await
        }
        Command::InternalCompletion { shell } => handle_internal_completion(&shell),
        Command::List { all, raw } => {
            let project = if all {
                None
            } else {
                Some(load_project_name(
                    args.config.clone(),
                    args.profile.clone(),
                )?)
            };
            vm_ops::handle_list_enhanced(None, project.as_deref(), raw)
        }
        Command::Create { environment, force } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_create(
                provider,
                config,
                global_config,
                force,
                subject.target,
                false,
                None,
                None,
                true,
                false,
            )
            .await
        }
        Command::Start {
            environment,
            no_wait,
        } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_start(
                provider,
                subject.target.as_deref(),
                config,
                global_config,
                no_wait,
            )
            .await
        }
        Command::Run {
            kind,
            words,
            provider,
            image,
            build,
            from_snapshot,
            ephemeral,
            mount,
            cpu,
            memory,
        } => {
            let name = parse_optional_as_name(&words)?;
            handle_run(RunIntent {
                kind,
                name,
                provider_override: provider,
                image,
                build,
                from_snapshot,
                ephemeral,
                mounts: mount,
                cpu,
                memory,
                config_path: args.config,
                profile: args.profile,
            })
            .await
        }
        Command::Shell {
            environment,
            path,
            command,
            force_refresh,
            no_refresh,
        } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, _) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            let command =
                command.map(|command| vec!["/bin/sh".to_string(), "-c".to_string(), command]);
            vm_ops::handle_ssh(
                provider,
                subject.target.as_deref(),
                path,
                command,
                config,
                force_refresh,
                no_refresh,
            )
        }
        Command::Exec {
            environment,
            command,
        } => {
            let (provider, config, _) = load_provider_context(args.config, args.profile, None)?;
            vm_ops::handle_exec(provider, Some(environment.as_str()), command, config)
        }
        Command::Logs {
            environment,
            follow,
            tail,
            service,
        } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, _) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_logs(
                provider,
                subject.target.as_deref(),
                config,
                follow,
                tail,
                service.as_deref(),
            )
        }
        Command::Copy {
            source,
            destination,
        } => {
            let (provider, config, _) = load_provider_context(args.config, args.profile, None)?;
            vm_ops::handle_copy(provider, &source, &destination, false, config)
        }
        Command::Stop { environment } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_stop(provider, subject.target.as_deref(), config, global_config).await
        }
        Command::Status { environment } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            if subject.target.is_none() {
                let project = load_project_name(args.config, subject.profile)?;
                return vm_ops::handle_list_enhanced(None, Some(project).as_deref(), false);
            }

            let (provider, _config, _) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            let report = provider
                .get_status_report(subject.target.as_deref())
                .map_err(VmError::from)?;
            vm_println!(
                "{}\t{}\t{}",
                report.name,
                report.provider,
                if report.is_running {
                    "running"
                } else {
                    "stopped"
                }
            );
            Ok(())
        }
        Command::Restart { environment } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_restart(provider, subject.target.as_deref(), config, global_config).await
        }
        Command::Remove { environment, force } => {
            let subject = resolve_environment(args.config.clone(), args.profile, environment)?;
            let (provider, config, global_config) =
                load_provider_context(args.config, subject.profile, subject.provider_override)?;
            vm_ops::handle_destroy_enhanced(
                provider,
                subject.target.as_deref(),
                config,
                global_config,
                &force,
                &false,
                &false,
                None,
                None,
                true,
            )
            .await
        }
        Command::Save {
            words,
            description,
            quiesce,
            force,
        } => {
            let (environment, snapshot) = parse_save_words(&words)?;
            handle_save(
                args.config,
                args.profile,
                environment,
                snapshot,
                description,
                quiesce,
                force,
            )
            .await
        }
        Command::Revert { words, force } => {
            let (environment, snapshot) = parse_revert_words(&words)?;
            handle_revert(args.config, args.profile, environment, snapshot, force).await
        }
        Command::Package {
            environment,
            output,
            compress,
            build,
        } => {
            handle_package(
                args.config,
                args.profile,
                environment,
                output,
                compress,
                build,
            )
            .await
        }
        Command::Tunnel { command } => handle_tunnel_command(command, args.config, args.profile),
        Command::GetSyncDirectory => {
            let (provider, _, _) = load_provider_context(args.config, args.profile, None)?;
            vm_ops::handle_get_sync_directory(provider);
            Ok(())
        }
    }
}

struct RunIntent {
    kind: EnvironmentKind,
    name: Option<String>,
    provider_override: Option<String>,
    image: Option<String>,
    build: Option<std::path::PathBuf>,
    from_snapshot: Option<String>,
    ephemeral: bool,
    mounts: Vec<String>,
    cpu: Option<String>,
    memory: Option<String>,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
}

async fn handle_run(intent: RunIntent) -> VmResult<()> {
    if intent.ephemeral || !intent.mounts.is_empty() {
        return handle_ephemeral_run(intent);
    }

    ensure_config_exists(
        intent.config_path.as_ref(),
        intent
            .provider_override
            .as_deref()
            .or(Some(intent.kind.default_provider())),
    )?;
    let provider_override = intent
        .provider_override
        .clone()
        .or_else(|| Some(intent.kind.default_provider().to_string()));
    let profile = profile_for_kind(&intent)?;
    let app_config = AppConfig::load(
        intent.config_path.clone(),
        profile,
        provider_override.clone(),
    )?;
    let mut config = app_config.vm;
    config.provider = provider_override;
    apply_run_overrides(&mut config, &intent)?;
    apply_kind_overrides(&mut config, &intent);
    let global_config = app_config.global;
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    let target = run_target(&intent);
    let connect_hint = shell_hint(&intent);

    let status = provider.get_status_report(target.as_deref());
    if status.as_ref().is_ok_and(|report| report.is_running) {
        vm_println!("✓ Environment is already running");
        vm_println!("  Connect with: {}", connect_hint);
        return Ok(());
    }

    let result = if status.is_ok() {
        vm_ops::handle_start(provider, target.as_deref(), config, global_config, false).await
    } else {
        vm_ops::handle_create(
            provider,
            config,
            global_config,
            false,
            target,
            false,
            None,
            intent.build,
            true,
            false,
        )
        .await
    };

    if result.is_ok() {
        vm_println!("  Connect with: {}", connect_hint);
    }

    result
}

fn run_target(intent: &RunIntent) -> Option<String> {
    intent.name.clone().or_else(|| {
        if intent.kind == EnvironmentKind::Mac {
            Some("mac".to_string())
        } else {
            None
        }
    })
}

fn profile_for_kind(intent: &RunIntent) -> VmResult<Option<String>> {
    if intent.profile.is_some() {
        return Ok(intent.profile.clone());
    }

    if intent.kind == EnvironmentKind::Mac {
        return Ok(mac_profile(intent.config_path.clone()));
    }

    Ok(None)
}

fn load_project_name(
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<String> {
    let app_config = AppConfig::load(config_path, profile, None)?;
    Ok(app_config
        .vm
        .project
        .as_ref()
        .and_then(|project| project.name.clone())
        .unwrap_or_else(|| "vm-project".to_string()))
}

fn shell_hint(intent: &RunIntent) -> String {
    match &intent.name {
        Some(name) => format!("vm shell {name}"),
        None => {
            let kind = match intent.kind {
                EnvironmentKind::Mac => "mac",
                EnvironmentKind::Linux => "linux",
                EnvironmentKind::Container => "container",
            };
            format!("vm shell {kind}")
        }
    }
}

fn handle_ephemeral_run(intent: RunIntent) -> VmResult<()> {
    use vm_temp::TempVmOps;

    let provider_override = intent
        .provider_override
        .clone()
        .or_else(|| Some(intent.kind.default_provider().to_string()));
    let mut config = load_config_lenient(intent.config_path)?;
    config.provider = provider_override;
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    TempVmOps::create(intent.mounts, intent.ephemeral, config, provider).map_err(VmError::from)
}

fn load_config_lenient(config_path: Option<std::path::PathBuf>) -> VmResult<VmConfig> {
    let config_file = config_path.unwrap_or_else(|| std::path::Path::new("vm.yaml").to_path_buf());
    if config_file.exists() {
        return VmConfig::from_file(&config_file).map_err(VmError::from);
    }

    const DEFAULTS: &str = include_str!("../../../../configs/defaults.yaml");
    let mut config: VmConfig = serde_yaml_ng::from_str(DEFAULTS)
        .map_err(|e| VmError::config(e, "Failed to parse embedded defaults"))?;
    if config.provider.is_none() {
        config.provider = Some("docker".to_string());
    }
    Ok(config)
}

fn ensure_config_exists(
    config_path: Option<&std::path::PathBuf>,
    provider: Option<&str>,
) -> VmResult<()> {
    let path = config_path
        .cloned()
        .unwrap_or_else(|| std::path::Path::new("vm.yaml").to_path_buf());
    if path.exists() {
        return Ok(());
    }

    Ok(init::handle_init(
        None,
        None,
        None,
        provider.map(ToString::to_string),
    )?)
}

fn apply_run_overrides(config: &mut VmConfig, intent: &RunIntent) -> VmResult<()> {
    let mut settings = config.vm.take().unwrap_or_default();
    if let Some(snapshot) = &intent.from_snapshot {
        settings.r#box = Some(BoxSpec::String(format!(
            "@{}",
            snapshot.trim_start_matches('@')
        )));
    } else if let Some(build_path) = &intent.build {
        settings.r#box = Some(BoxSpec::String(build_path.to_string_lossy().to_string()));
    } else if let Some(image) = &intent.image {
        settings.r#box = Some(BoxSpec::String(image.clone()));
    }
    if let Some(cpu) = &intent.cpu {
        settings.cpus = Some(parse_cpu_limit(cpu)?);
    }
    if let Some(memory) = &intent.memory {
        settings.memory = Some(parse_memory_limit(memory)?);
    }
    config.vm = Some(settings);
    Ok(())
}

fn apply_kind_overrides(config: &mut VmConfig, intent: &RunIntent) {
    if intent.kind != EnvironmentKind::Mac {
        return;
    }

    let tart = config.tart.get_or_insert_with(TartConfig::default);
    tart.guest_os = Some("macos".to_string());
    tart.ssh_user.get_or_insert_with(|| "admin".to_string());

    if intent.image.is_some() || intent.build.is_some() || intent.from_snapshot.is_some() {
        return;
    }

    let settings = config.vm.get_or_insert_with(Default::default);
    let should_replace_box = match settings.r#box.as_ref() {
        None => true,
        Some(BoxSpec::String(value)) => {
            value == "ubuntu:jammy"
                || value == "ubuntu:24.04"
                || value == "vibe-tart-linux-base"
                || value == "@vibe-box"
                || value == "vibe-tart-base"
        }
        Some(_) => false,
    };

    if should_replace_box {
        settings.r#box = Some(BoxSpec::String(
            "ghcr.io/cirruslabs/macos-sequoia-base:latest".to_string(),
        ));
    }
}

fn parse_cpu_limit(value: &str) -> VmResult<CpuLimit> {
    serde_yaml_ng::from_str(value).map_err(|e| {
        VmError::validation(
            format!("Invalid CPU limit '{}': {}", value, e),
            Some("Use a count like 4, a percentage like 50%, or unlimited.".to_string()),
        )
    })
}

fn parse_memory_limit(value: &str) -> VmResult<MemoryLimit> {
    serde_yaml_ng::from_str(value).map_err(|e| {
        VmError::validation(
            format!("Invalid memory limit '{}': {}", value, e),
            Some("Use a value like 8192, 8g, 50%, or unlimited.".to_string()),
        )
    })
}

fn load_provider_context(
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
    provider_override: Option<String>,
) -> VmResult<(
    Box<dyn vm_provider::Provider>,
    VmConfig,
    vm_config::GlobalConfig,
)> {
    let app_config = AppConfig::load(config_path, profile, provider_override)?;
    let config = app_config.vm;
    let global_config = app_config.global;
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    Ok((provider, config, global_config))
}

fn handle_tunnel_command(
    command: TunnelSubcommand,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let (provider, config, global_config) = load_provider_context(config_path, profile, None)?;
    match command {
        TunnelSubcommand::Add {
            mapping,
            environment,
        } => tunnel::handle_tunnel(
            provider,
            &mapping,
            environment.as_deref(),
            config,
            global_config,
        ),
        TunnelSubcommand::Ls { environment } => {
            tunnel::handle_tunnel_list(provider, environment.as_deref(), config, global_config)
        }
        TunnelSubcommand::Stop {
            port,
            environment,
            all,
        } => tunnel::handle_tunnel_stop(
            provider,
            port,
            environment.as_deref(),
            all,
            config,
            global_config,
        ),
    }
}

async fn handle_secret_command(
    command: &SecretSubcommand,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    let global_config = AppConfig::load(config_path, profile, None)
        .map(|config| config.global)
        .unwrap_or_default();
    secrets::handle_secrets_command(command, global_config).await
}

async fn handle_system_command(
    command: &SystemSubcommand,
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
) -> VmResult<()> {
    match command {
        SystemSubcommand::Update { version, force } => {
            update::handle_update(version.as_deref(), *force)
        }
        SystemSubcommand::Uninstall { keep_config, yes } => {
            uninstall::handle_uninstall(*keep_config, *yes)
        }
        SystemSubcommand::Registry { command } => {
            let global_config = AppConfig::load(config_path, profile, None)
                .map(|config| config.global)
                .unwrap_or_default();
            registry::handle_registry_command(command, global_config).await
        }
        SystemSubcommand::Base { command } => base::handle_base(command.clone()).await,
    }
}

async fn handle_save(
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    snapshot: String,
    description: Option<String>,
    quiesce: bool,
    force: bool,
) -> VmResult<()> {
    let app_config = AppConfig::load(config_path, profile, None)?;
    let executable = app_config.vm.provider.as_deref().unwrap_or("docker");
    let project =
        environment.or_else(|| app_config.vm.project.as_ref().and_then(|p| p.name.clone()));
    vm_snapshot::create::handle_create(
        &app_config,
        executable,
        &snapshot,
        description.as_deref(),
        quiesce,
        project.as_deref(),
        None,
        None,
        &[],
        force,
    )
    .await
    .map_err(VmError::from)
}

async fn handle_revert(
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    snapshot: String,
    force: bool,
) -> VmResult<()> {
    let app_config = AppConfig::load(config_path, profile, None)?;
    let executable = app_config.vm.provider.as_deref().unwrap_or("docker");
    let project =
        environment.or_else(|| app_config.vm.project.as_ref().and_then(|p| p.name.clone()));
    vm_snapshot::restore::handle_restore(
        &app_config,
        executable,
        &snapshot,
        project.as_deref(),
        force,
    )
    .await
    .map_err(VmError::from)
}

async fn handle_package(
    config_path: Option<std::path::PathBuf>,
    profile: Option<String>,
    environment: Option<String>,
    output: Option<std::path::PathBuf>,
    compress: u8,
    build: Option<std::path::PathBuf>,
) -> VmResult<()> {
    let app_config = AppConfig::load(config_path, profile, None)?;
    let executable = app_config.vm.provider.as_deref().unwrap_or("docker");
    let project =
        environment.or_else(|| app_config.vm.project.as_ref().and_then(|p| p.name.clone()));
    let snapshot = project.as_deref().unwrap_or("environment");

    if let Some(dockerfile) = build {
        vm_snapshot::create::handle_create(
            &app_config,
            executable,
            snapshot,
            Some("Portable base image"),
            false,
            project.as_deref(),
            Some(&dockerfile),
            Some(std::path::Path::new(".")),
            &[],
            true,
        )
        .await?;
    }

    vm_snapshot::export::handle_export(
        executable,
        snapshot,
        output.as_deref(),
        compress,
        project.as_deref(),
    )
    .await
    .map_err(VmError::from)
}

fn parse_optional_as_name(words: &[String]) -> VmResult<Option<String>> {
    match words {
        [] => Ok(None),
        [as_word, name] if as_word == "as" => Ok(Some(name.clone())),
        _ => Err(VmError::validation(
            "Invalid naming syntax".to_string(),
            Some("Use: vm run linux as backend".to_string()),
        )),
    }
}

fn parse_save_words(words: &[String]) -> VmResult<(Option<String>, String)> {
    match words {
        [as_word, snapshot] if as_word == "as" => Ok((None, snapshot.clone())),
        [environment, as_word, snapshot] if as_word == "as" => {
            Ok((Some(environment.clone()), snapshot.clone()))
        }
        _ => Err(VmError::validation(
            "Invalid save syntax".to_string(),
            Some("Use: vm save as stable or vm save backend as stable".to_string()),
        )),
    }
}

fn parse_revert_words(words: &[String]) -> VmResult<(Option<String>, String)> {
    match words {
        [snapshot] => Ok((None, snapshot.clone())),
        [environment, snapshot] => Ok((Some(environment.clone()), snapshot.clone())),
        _ => Err(VmError::validation(
            "Invalid revert syntax".to_string(),
            Some("Use: vm revert stable or vm revert backend stable".to_string()),
        )),
    }
}

fn print_dry_run_summary(args: &Args) {
    vm_println!("{}", MESSAGES.vm.dry_run_header);
    vm_println!("  Command: {:?}", args.command);
    if let Some(config) = &args.config {
        vm_println!("  Config: {}", config.display());
    }
    vm_println!("{}", MESSAGES.vm.dry_run_complete);
}

fn handle_plugin_command(command: &PluginSubcommand) -> VmResult<()> {
    match command {
        PluginSubcommand::Ls => plugin::handle_plugin_list().map_err(VmError::from),
        PluginSubcommand::Info { plugin_name } => {
            plugin::handle_plugin_info(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::Install { source_path } => {
            plugin::handle_plugin_install(source_path).map_err(VmError::from)
        }
        PluginSubcommand::Rm { plugin_name } => {
            plugin::handle_plugin_remove(plugin_name).map_err(VmError::from)
        }
        PluginSubcommand::New {
            plugin_name,
            r#type,
        } => plugin_new::handle_plugin_new(plugin_name, r#type).map_err(VmError::from),
        PluginSubcommand::Validate { plugin_name } => {
            plugin::handle_plugin_validate(plugin_name).map_err(VmError::from)
        }
    }
}

fn handle_internal_completion(shell: &str) -> VmResult<()> {
    use clap::CommandFactory;
    use clap_complete::{generate, shells};
    use std::io::{self, Write};

    let mut cmd = crate::cli::Args::command();
    let mut stdout = io::stdout();

    match shell.to_lowercase().as_str() {
        "bash" => {
            generate(shells::Bash, &mut cmd, "vm", &mut stdout);
            Ok(())
        }
        "zsh" => {
            stdout.write_all(ZSH_COMPLETION_PRELUDE.as_bytes())?;
            generate(shells::Zsh, &mut cmd, "vm", &mut stdout);
            Ok(())
        }
        "fish" => {
            generate(shells::Fish, &mut cmd, "vm", &mut stdout);
            Ok(())
        }
        "powershell" => {
            generate(shells::PowerShell, &mut cmd, "vm", &mut stdout);
            Ok(())
        }
        _ => {
            vm_error!(
                "Unsupported shell: {}. Supported shells: bash, zsh, fish, powershell",
                shell
            );
            Err(VmError::general(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Unsupported shell"),
                format!(
                    "Shell '{}' is not supported. Use: bash, zsh, fish, or powershell",
                    shell
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_optional_as_name, parse_save_words, EnvironmentKind, RunIntent,
        ZSH_COMPLETION_PRELUDE,
    };

    #[test]
    fn zsh_completion_prelude_initializes_compdef_for_direct_sourcing() {
        assert!(ZSH_COMPLETION_PRELUDE.contains("${functions[compdef]+x}"));
        assert!(ZSH_COMPLETION_PRELUDE.contains("autoload -Uz compinit"));
        assert!(ZSH_COMPLETION_PRELUDE.contains("compinit -i"));
    }

    #[test]
    fn parses_humane_run_name() {
        assert_eq!(
            parse_optional_as_name(&["as".into(), "backend".into()]).unwrap(),
            Some("backend".into())
        );
    }

    #[test]
    fn rejects_non_humane_run_name() {
        assert!(parse_optional_as_name(&["backend".into()]).is_err());
    }

    #[test]
    fn parses_save_target_and_snapshot() {
        assert_eq!(
            parse_save_words(&["backend".into(), "as".into(), "stable".into()]).unwrap(),
            (Some("backend".into()), "stable".into())
        );
    }

    #[test]
    fn shell_hint_uses_kind_when_run_has_no_name() {
        let intent = RunIntent {
            kind: EnvironmentKind::Mac,
            name: None,
            provider_override: None,
            image: None,
            build: None,
            from_snapshot: None,
            ephemeral: false,
            mounts: vec![],
            cpu: None,
            memory: None,
            config_path: None,
            profile: None,
        };
        assert_eq!(super::shell_hint(&intent), "vm shell mac");
    }
}
