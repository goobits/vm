use std::collections::BTreeMap;

use vm_config::{config::ImageSpec, config::VmConfig};
use vm_provider::CommandProvider;

use crate::error::{VmError, VmResult};

const VENDOR_PROBE: &str = include_str!("runtime/vendor-probe.sh");

const VENDOR_REPAIR: &str = include_str!("runtime/vendor-repair.sh");

const VENDOR_RECONCILE_WORKER: &str = include_str!("runtime/vendor-reconcile-worker.sh");

const VENDOR_RECONCILE_LAUNCHER: &str = include_str!("runtime/vendor-reconcile-launcher.sh");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::commands) enum VendorToolState {
    Absent,
    Incomplete,
    Consumable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::commands) struct VendorToolStatus {
    pub(in crate::commands) state: VendorToolState,
    pub(in crate::commands) version: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub(in crate::commands) struct VendorToolInfo {
    pub(in crate::commands) name: &'static str,
    pub(in crate::commands) installer_url: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum VendorLayout {
    Package { marker: &'static str },
    Binary,
}

impl VendorLayout {
    fn name(self) -> &'static str {
        match self {
            Self::Package { .. } => "package",
            Self::Binary => "binary",
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::Package { marker } => marker,
            Self::Binary => "",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct VendorToolDefinition {
    name: &'static str,
    primary: &'static str,
    installed_path: &'static str,
    installer_url: &'static str,
    installer_shell: &'static str,
    installer_args: &'static [&'static str],
    layout: VendorLayout,
    required: &'static [&'static str],
    approved_user_scope: &'static str,
}

const VENDOR_TOOLS: &[VendorToolDefinition] = &[
    VendorToolDefinition {
        name: "antigravity",
        primary: "agy",
        installed_path: ".local/bin/agy",
        installer_url: "https://antigravity.google/cli/install.sh",
        installer_shell: "bash",
        installer_args: &[],
        layout: VendorLayout::Binary,
        required: &["agy"],
        approved_user_scope: "file:.local/bin/agy",
    },
    VendorToolDefinition {
        name: "claude",
        primary: "claude",
        installed_path: ".local/bin/claude",
        installer_url: "https://claude.ai/install.sh",
        installer_shell: "bash",
        installer_args: &["stable"],
        layout: VendorLayout::Binary,
        required: &["claude"],
        approved_user_scope: "symlink:.local/share/claude/versions",
    },
    VendorToolDefinition {
        name: "codex",
        primary: "codex",
        installed_path: ".codex/packages/standalone/current/bin/codex",
        installer_url: "https://chatgpt.com/codex/install.sh",
        installer_shell: "sh",
        installer_args: &[],
        layout: VendorLayout::Package {
            marker: "codex-package.json",
        },
        required: &["codex", "codex-code-mode-host"],
        approved_user_scope: "symlink:.codex/packages/standalone",
    },
];

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

#[derive(Debug, Clone, Copy)]
enum ReconcileAction {
    Repair,
    Update,
}

impl ReconcileAction {
    fn as_str(self) -> &'static str {
        match self {
            Self::Repair => "repair",
            Self::Update => "update",
        }
    }
}

pub(in crate::commands) fn reconcile_vendor_tools(
    provider: &dyn CommandProvider,
    environment: &str,
    config: &VmConfig,
) -> VmResult<()> {
    let expected = vendor_tools_expected(config);
    for definition in VENDOR_TOOLS {
        launch_reconciliation(
            provider,
            environment,
            definition,
            expected,
            ReconcileMode::Wait,
            ReconcileAction::Repair,
        )?;
    }
    Ok(())
}

pub(in crate::commands) fn update_vendor_tools(
    provider: &dyn CommandProvider,
    environment: &str,
    config: &VmConfig,
    selected: &[String],
    all_expected: bool,
    background: bool,
) -> VmResult<()> {
    if all_expected && !vendor_tools_expected(config) {
        return Ok(());
    }
    let mode = if background {
        ReconcileMode::Background
    } else {
        ReconcileMode::Wait
    };
    for definition in VENDOR_TOOLS {
        if !all_expected && !selected.iter().any(|name| name == definition.name) {
            continue;
        }
        launch_reconciliation(
            provider,
            environment,
            definition,
            true,
            mode,
            ReconcileAction::Update,
        )?;
    }
    Ok(())
}

fn launch_reconciliation(
    provider: &dyn CommandProvider,
    environment: &str,
    definition: &VendorToolDefinition,
    expected: bool,
    mode: ReconcileMode,
    action: ReconcileAction,
) -> VmResult<()> {
    let mut command = vec![
        "sh".into(),
        "-c".into(),
        VENDOR_RECONCILE_LAUNCHER.into(),
        "vm-vendor-reconcile-launcher".into(),
        VENDOR_PROBE.into(),
        VENDOR_REPAIR.into(),
        VENDOR_RECONCILE_WORKER.into(),
        mode.as_str().into(),
        action.as_str().into(),
        if expected { "yes" } else { "no" }.into(),
        environment.into(),
        definition.name.into(),
        definition.primary.into(),
        definition.installed_path.into(),
        definition.layout.name().into(),
        definition.layout.marker().into(),
        definition.required.join(","),
        "/usr/local".into(),
        String::new(),
        definition.installer_url.into(),
        definition.installer_shell.into(),
        definition.approved_user_scope.into(),
    ];
    command.extend(definition.installer_args.iter().map(ToString::to_string));
    provider
        .exec(Some(environment), &command)
        .map_err(VmError::from)
}

pub(in crate::commands) fn vendor_tool_statuses(
    provider: &dyn CommandProvider,
    environment: &str,
) -> VmResult<BTreeMap<&'static str, VendorToolStatus>> {
    let mut statuses = BTreeMap::new();
    for definition in VENDOR_TOOLS {
        let output = provider
            .exec_output(
                Some(environment),
                &[
                    "sh".into(),
                    "-c".into(),
                    VENDOR_PROBE.into(),
                    "vm-vendor-probe".into(),
                    definition.name.into(),
                    definition.primary.into(),
                    definition.layout.name().into(),
                    definition.layout.marker().into(),
                    definition.required.join(","),
                ],
            )
            .map_err(VmError::from)?;
        statuses.insert(definition.name, parse_vendor_tool_status(&output)?);
    }
    Ok(statuses)
}

fn parse_vendor_tool_status(output: &str) -> VmResult<VendorToolStatus> {
    let state = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("VM_VENDOR_TOOL_STATE="))
        .unwrap_or_default();
    let state = match state {
        "absent" => VendorToolState::Absent,
        "incomplete" => VendorToolState::Incomplete,
        "consumable" => VendorToolState::Consumable,
        value => {
            return Err(VmError::validation(
                format!("Vendor-tool runtime probe returned an unknown state '{value}'"),
                None::<String>,
            ))
        }
    };
    let version = output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("VM_VENDOR_TOOL_VERSION=")
            .filter(|version| !version.is_empty())
            .map(ToString::to_string)
    });
    Ok(VendorToolStatus { state, version })
}

pub(in crate::commands) fn vendor_tools_expected(config: &VmConfig) -> bool {
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

pub(in crate::commands) fn is_vendor_tool(name: &str) -> bool {
    VENDOR_TOOLS.iter().any(|tool| tool.name == name)
}

pub(in crate::commands) fn vendor_tool_info() -> impl Iterator<Item = VendorToolInfo> {
    VENDOR_TOOLS.iter().map(|tool| VendorToolInfo {
        name: tool.name,
        installer_url: tool.installer_url,
    })
}

#[cfg(test)]
mod tests;
