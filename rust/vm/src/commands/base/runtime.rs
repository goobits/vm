use vm_config::{config::ImageSpec, config::VmConfig};
use vm_provider::CommandProvider;

use crate::error::{VmError, VmResult};

const CODEX_PROBE: &str = include_str!("runtime/codex-probe.sh");

const CODEX_REPAIR: &str = include_str!("runtime/codex-repair.sh");

const CODEX_RECONCILE_WORKER: &str = include_str!("runtime/codex-reconcile-worker.sh");

const CODEX_RECONCILE_LAUNCHER: &str = include_str!("runtime/codex-reconcile-launcher.sh");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::commands) enum CodexState {
    Absent,
    Incomplete,
    Consumable,
}

#[derive(Debug, Clone, Copy)]
enum ReconcileMode {
    Background,
    Wait,
}

impl ReconcileMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Wait => "wait",
        }
    }
}

pub(in crate::commands) fn reconcile_codex(
    provider: &dyn CommandProvider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    launch_reconciliation(
        provider,
        environment,
        codex_expected(config),
        ReconcileMode::Wait,
    )
}

pub(in crate::commands) fn reconcile_codex_in_background(
    provider: &dyn CommandProvider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<bool> {
    if !codex_expected(config) {
        return Ok(false);
    }
    launch_reconciliation(provider, environment, true, ReconcileMode::Background)?;
    Ok(true)
}

fn launch_reconciliation(
    provider: &dyn CommandProvider,
    environment: &str,
    expected: bool,
    mode: ReconcileMode,
) -> VmResult<()> {
    provider
        .exec(
            Some(environment),
            &[
                "sh".into(),
                "-c".into(),
                CODEX_RECONCILE_LAUNCHER.into(),
                "vm-codex-reconcile-launcher".into(),
                CODEX_PROBE.into(),
                CODEX_REPAIR.into(),
                CODEX_RECONCILE_WORKER.into(),
                mode.as_str().into(),
                if expected { "yes" } else { "no" }.into(),
                environment.into(),
            ],
        )
        .map_err(VmError::from)
}

pub(in crate::commands) fn codex_state(
    provider: &dyn CommandProvider,
    environment: &str,
) -> VmResult<CodexState> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), CODEX_PROBE.into()],
        )
        .map_err(VmError::from)?;
    parse_codex_state(&output)
}

fn parse_codex_state(output: &str) -> VmResult<CodexState> {
    let state = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("VM_CODEX_STATE="))
        .unwrap_or_default();
    match state {
        "absent" => Ok(CodexState::Absent),
        "incomplete" => Ok(CodexState::Incomplete),
        "consumable" => Ok(CodexState::Consumable),
        value => Err(VmError::validation(
            format!("Codex runtime probe returned an unknown state '{value}'"),
            None::<String>,
        )),
    }
}

pub(in crate::commands) fn codex_expected(config: &VmConfig) -> bool {
    config.preset.as_deref().is_some_and(|presets| {
        presets
            .split(',')
            .any(|preset| preset.trim().eq_ignore_ascii_case("vibe"))
    }) || config
        .vm
        .as_ref()
        .and_then(|settings| settings.image.clone())
        .is_some_and(|spec| match spec {
            ImageSpec::String(name) => name.to_ascii_lowercase().contains("vibe"),
            ImageSpec::Build { .. } => false,
        })
}

#[cfg(test)]
mod tests;
