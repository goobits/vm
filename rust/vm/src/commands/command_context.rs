//! Configuration, provider, and target assembly for command handlers.

use std::path::PathBuf;

use super::environment::resolve_environment;
use super::vm_ops;
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, AppConfig, GlobalConfig};
use vm_core::vm_progress;
use vm_provider::{get_provider, Provider};

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
