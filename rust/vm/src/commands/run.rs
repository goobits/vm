use std::path::{Path, PathBuf};

use crate::cli::EnvironmentKind;
use crate::error::{VmError, VmResult};
use vm_config::{
    config::{BoxSpec, CpuLimit, MemoryLimit, TartConfig, VmConfig},
    AppConfig,
};
use vm_core::{vm_hint, vm_progress, vm_success};
use vm_provider::{get_provider, InstanceState, VmError as ProviderError};

use super::{environment::mac_profile, vm_ops};

pub(super) struct RunIntent {
    pub kind: EnvironmentKind,
    pub name: Option<String>,
    pub provider_override: Option<String>,
    pub image: Option<String>,
    pub build: Option<PathBuf>,
    pub from_snapshot: Option<String>,
    pub ephemeral: bool,
    pub mounts: Vec<String>,
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub config_path: Option<PathBuf>,
    pub profile: Option<String>,
}

pub(super) async fn handle(intent: RunIntent) -> VmResult<()> {
    if intent.ephemeral || !intent.mounts.is_empty() {
        return handle_ephemeral(intent);
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
    let app_config = AppConfig::load(
        intent.config_path.clone(),
        profile_for_kind(&intent),
        provider_override.clone(),
    )?;
    let mut config = app_config.vm;
    config.provider = provider_override.map(Into::into);
    apply_overrides(&mut config, &intent)?;
    apply_kind(&mut config, &intent);
    super::packages::apply_client_environment(&mut config)?;

    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    let target = target(&intent);
    let connect_hint = shell_hint(&intent);
    match provider.instance_state(target.as_deref()) {
        Ok(InstanceState::Running) => {
            vm_success!("Environment is already running");
            vm_hint!("Connect with: {connect_hint}");
            Ok(())
        }
        Ok(_) => {
            vm_ops::handle_start(
                provider,
                target.as_deref(),
                config,
                app_config.global,
                false,
            )
            .await
        }
        Err(ProviderError::NotFound(_)) => {
            vm_ops::handle_create(provider, config, app_config.global, false, target).await
        }
        Err(error) => Err(VmError::from(error)),
    }
}

pub(super) fn parse_name(words: &[String]) -> VmResult<Option<String>> {
    match words {
        [] => Ok(None),
        [as_word, name] if as_word == "as" => Ok(Some(name.clone())),
        _ => Err(VmError::validation(
            "Invalid naming syntax".to_string(),
            Some("Use: vm run linux as backend".to_string()),
        )),
    }
}

fn target(intent: &RunIntent) -> Option<String> {
    intent
        .name
        .clone()
        .or_else(|| (intent.kind == EnvironmentKind::Mac).then(|| "mac".to_string()))
}

fn profile_for_kind(intent: &RunIntent) -> Option<String> {
    intent.profile.clone().or_else(|| {
        (intent.kind == EnvironmentKind::Mac)
            .then(|| mac_profile(intent.config_path.clone()))
            .flatten()
    })
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

fn handle_ephemeral(intent: RunIntent) -> VmResult<()> {
    use vm_temp::TempVmOps;

    let provider_override = intent
        .provider_override
        .clone()
        .or_else(|| Some(intent.kind.default_provider().to_string()));
    let mut config = load_config_lenient(intent.config_path)?;
    config.provider = provider_override.map(Into::into);
    let provider = get_provider(config.clone()).map_err(VmError::from)?;
    vm_progress!("Creating temporary environment...");
    TempVmOps::create(intent.mounts, intent.ephemeral, config, provider).map_err(VmError::from)?;
    vm_success!("Temporary environment created");
    Ok(())
}

fn load_config_lenient(config_path: Option<PathBuf>) -> VmResult<VmConfig> {
    let config_file = config_path.unwrap_or_else(|| Path::new("vm.yaml").to_path_buf());
    if config_file.exists() {
        return VmConfig::from_file(&config_file).map_err(VmError::from);
    }

    const DEFAULTS: &str = include_str!("../../../../configs/defaults.yaml");
    let mut config: VmConfig = serde_yaml_ng::from_str(DEFAULTS)
        .map_err(|error| VmError::config(error, "Failed to parse embedded defaults"))?;
    config.provider.get_or_insert_with(Default::default);
    Ok(config)
}

fn ensure_config_exists(config_path: Option<&PathBuf>, provider: Option<&str>) -> VmResult<()> {
    let path = config_path
        .cloned()
        .unwrap_or_else(|| Path::new("vm.yaml").to_path_buf());
    if path.exists() {
        return Ok(());
    }

    vm_config::init_config_file(None, None, None, provider.map(ToString::to_string))
        .map_err(|error| VmError::config(error, "initialize project configuration"))
}

fn apply_overrides(config: &mut VmConfig, intent: &RunIntent) -> VmResult<()> {
    let mut settings = config.vm.take().unwrap_or_default();
    settings.r#box = intent
        .from_snapshot
        .as_ref()
        .map(|snapshot| BoxSpec::String(format!("@{}", snapshot.trim_start_matches('@'))))
        .or_else(|| {
            intent
                .build
                .as_ref()
                .map(|path| BoxSpec::String(path.to_string_lossy().to_string()))
        })
        .or_else(|| intent.image.clone().map(BoxSpec::String))
        .or(settings.r#box);
    if let Some(cpu) = &intent.cpu {
        settings.cpus = Some(parse_cpu_limit(cpu)?);
    }
    if let Some(memory) = &intent.memory {
        settings.memory = Some(parse_memory_limit(memory)?);
    }
    config.vm = Some(settings);
    Ok(())
}

fn apply_kind(config: &mut VmConfig, intent: &RunIntent) {
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
    let replace_default = match settings.r#box.as_ref() {
        None => true,
        Some(BoxSpec::String(value)) => matches!(
            value.as_str(),
            "ubuntu:jammy"
                | "ubuntu:24.04"
                | "vibe-tart-linux-base"
                | "@vibe-box"
                | "vibe-tart-base"
        ),
        Some(_) => false,
    };
    if replace_default {
        settings.r#box = Some(BoxSpec::String(
            "ghcr.io/cirruslabs/macos-sequoia-base:latest".to_string(),
        ));
    }
}

fn parse_cpu_limit(value: &str) -> VmResult<CpuLimit> {
    serde_yaml_ng::from_str(value).map_err(|error| {
        VmError::validation(
            format!("Invalid CPU limit '{value}': {error}"),
            Some("Use a count like 4, a percentage like 50%, or unlimited.".to_string()),
        )
    })
}

fn parse_memory_limit(value: &str) -> VmResult<MemoryLimit> {
    serde_yaml_ng::from_str(value).map_err(|error| {
        VmError::validation(
            format!("Invalid memory limit '{value}': {error}"),
            Some("Use a value like 8192, 8g, 50%, or unlimited.".to_string()),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{parse_name, shell_hint, EnvironmentKind, RunIntent};

    #[test]
    fn parses_humane_name() {
        assert_eq!(
            parse_name(&["as".into(), "backend".into()]).unwrap(),
            Some("backend".into())
        );
    }

    #[test]
    fn rejects_non_humane_name() {
        assert!(parse_name(&["backend".into()]).is_err());
    }

    #[test]
    fn shell_hint_uses_kind_without_a_name() {
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
        assert_eq!(shell_hint(&intent), "vm shell mac");
    }
}
