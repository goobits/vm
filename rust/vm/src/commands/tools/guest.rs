use std::collections::{BTreeMap, BTreeSet};

use vm_packages::{
    validate_sha256, validate_tool_name, validate_tool_target, validate_tool_version,
    RegistryEndpoints, ToolArtifactRecord, ToolKind,
};
use vm_provider::CommandProvider;

use crate::error::{VmError, VmResult};

const INSTALLER: &str = include_str!("guest/installer.sh");
const LAUNCHER: &str = include_str!("guest/launcher.sh");
const STATE_SCRIPT: &str = include_str!("guest/state.sh");
const CONSUMABLE_SCRIPT: &str = include_str!("guest/consumable.sh");
const PROJECT_COLLECTION_OVERRIDES_SCRIPT: &str =
    include_str!("guest/project-collection-overrides.sh");

const PLATFORM_SECTION: &str = "__VM_TOOL_PLATFORM__";
const INSTALLED_SECTION: &str = "__VM_TOOL_INSTALLED__";
const CONSUMABLE_SECTION: &str = "__VM_TOOL_CONSUMABLE__";

fn shell_state_script() -> String {
    format!(
        "printf '%s\\n' {PLATFORM_SECTION}; uname -s; uname -m; \
         printf '%s\\n' {INSTALLED_SECTION}; {STATE_SCRIPT}
         printf '%s\\n' {CONSUMABLE_SECTION}; {CONSUMABLE_SCRIPT}"
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InstalledTool {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) target: String,
    pub(super) digest: String,
}

pub(super) struct ShellState {
    pub(super) target: String,
    pub(super) installed: BTreeMap<String, InstalledTool>,
    pub(super) consumable: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstallMode {
    BackgroundIfIdle,
    Background,
    Wait,
}

impl InstallMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::BackgroundIfIdle => "background-if-idle",
            Self::Background => "background",
            Self::Wait => "wait",
        }
    }
}

pub(super) fn platform_target(
    provider: &dyn CommandProvider,
    environment: &str,
) -> VmResult<String> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), "uname -s; uname -m".into()],
        )
        .map_err(VmError::from)?;
    platform_target_from_uname(&output)
}

pub(super) fn installed(
    provider: &dyn CommandProvider,
    environment: &str,
) -> VmResult<BTreeMap<String, InstalledTool>> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), STATE_SCRIPT.into()],
        )
        .map_err(VmError::from)?;
    Ok(parse_installed(&output))
}

pub(super) fn consumable(
    provider: &dyn CommandProvider,
    environment: &str,
) -> VmResult<BTreeMap<String, bool>> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), CONSUMABLE_SCRIPT.into()],
        )
        .map_err(VmError::from)?;
    Ok(parse_consumable(&output))
}

pub(super) fn shell_state(
    provider: &dyn CommandProvider,
    environment: &str,
) -> VmResult<ShellState> {
    let script = shell_state_script();
    let output = provider
        .exec_output(Some(environment), &["sh".into(), "-c".into(), script])
        .map_err(VmError::from)?;
    parse_shell_state(&output)
}

pub(super) fn project_collection_overrides(
    provider: &dyn CommandProvider,
    environment: &str,
    workspace: &str,
    artifacts: &BTreeMap<String, ToolArtifactRecord>,
) -> VmResult<BTreeMap<String, BTreeSet<String>>> {
    let candidates = artifacts
        .values()
        .filter(|artifact| artifact.kind == ToolKind::Collection)
        .flat_map(|artifact| {
            artifact
                .links
                .keys()
                .cloned()
                .map(move |destination| (artifact.tool.clone(), destination))
        })
        .collect::<BTreeSet<_>>();
    if candidates.is_empty() {
        return Ok(BTreeMap::new());
    }

    let mut command = vec![
        "sh".to_string(),
        "-c".to_string(),
        PROJECT_COLLECTION_OVERRIDES_SCRIPT.to_string(),
        "vm-tool-project-overrides".to_string(),
        workspace.to_string(),
    ];
    for (name, destination) in &candidates {
        command.push(name.clone());
        command.push(destination.clone());
    }
    let output = provider
        .exec_output(Some(environment), &command)
        .map_err(VmError::from)?;
    Ok(parse_project_collection_overrides(&output, &candidates))
}

pub(super) fn install(
    provider: &dyn CommandProvider,
    environment: &str,
    artifacts: &[ToolArtifactRecord],
    gateway: &str,
    read_token: &str,
    mode: InstallMode,
) -> VmResult<()> {
    if artifacts.is_empty() {
        return Ok(());
    }
    let gateway = RegistryEndpoints::new(gateway).map_err(VmError::from)?;
    let mut command = vec![
        "sh".to_string(),
        "-c".to_string(),
        LAUNCHER.to_string(),
        "vm-tool-launcher".to_string(),
        INSTALLER.to_string(),
        mode.as_str().to_string(),
    ];
    for artifact in artifacts {
        command.push(manifest(artifact, gateway.gateway())?);
    }
    let input = format!("{read_token}\n");
    provider
        .exec_with_stdin(Some(environment), &command, input.as_bytes())
        .map_err(VmError::from)
}

fn manifest(artifact: &ToolArtifactRecord, gateway: &str) -> VmResult<String> {
    artifact.validate().map_err(VmError::from)?;
    let kind = match artifact.kind {
        vm_packages::ToolKind::Binary => "binary",
        vm_packages::ToolKind::Collection => "collection",
    };
    let mut manifest = format!(
        "{}\t{}\t{}\t{}\t{}\t{}{}",
        artifact.tool,
        artifact.version,
        artifact.target,
        kind,
        artifact.artifact_digest,
        gateway.trim_end_matches('/'),
        artifact.artifact_path
    );
    for (destination, source) in &artifact.links {
        manifest.push('\n');
        manifest.push_str(destination);
        manifest.push('\t');
        manifest.push_str(source);
    }
    Ok(manifest)
}

fn platform_target_from_uname(output: &str) -> VmResult<String> {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let system = lines.next().unwrap_or_default().to_ascii_lowercase();
    let architecture = lines.next().unwrap_or_default().to_ascii_lowercase();
    let os = match system.as_str() {
        "linux" => "linux",
        "darwin" => "darwin",
        _ => {
            return Err(VmError::validation(
                format!("Unsupported guest operating system '{system}'"),
                None::<String>,
            ));
        }
    };
    let architecture = match architecture.as_str() {
        "arm64" | "aarch64" => "arm64",
        "amd64" | "x86_64" => "amd64",
        _ => {
            return Err(VmError::validation(
                format!("Unsupported guest architecture '{architecture}'"),
                None::<String>,
            ));
        }
    };
    Ok(format!("{os}-{architecture}"))
}

fn parse_installed(output: &str) -> BTreeMap<String, InstalledTool> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != 4
                || validate_tool_name(fields[0]).is_err()
                || validate_tool_version(fields[1]).is_err()
                || validate_tool_target(fields[2]).is_err()
                || validate_sha256(fields[3]).is_err()
            {
                return None;
            }
            let tool = InstalledTool {
                name: fields[0].into(),
                version: fields[1].into(),
                target: fields[2].into(),
                digest: fields[3].into(),
            };
            Some((tool.name.clone(), tool))
        })
        .collect()
}

fn parse_consumable(output: &str) -> BTreeMap<String, bool> {
    output
        .lines()
        .filter_map(|line| {
            let (name, state) = line.split_once('\t')?;
            if validate_tool_name(name).is_err() {
                return None;
            }
            match state {
                "yes" => Some((name.to_string(), true)),
                "no" => Some((name.to_string(), false)),
                _ => None,
            }
        })
        .collect()
}

fn parse_shell_state(output: &str) -> VmResult<ShellState> {
    let mut section = "";
    let mut platform = String::new();
    let mut installed = String::new();
    let mut consumable = String::new();
    for line in output.lines() {
        match line {
            PLATFORM_SECTION | INSTALLED_SECTION | CONSUMABLE_SECTION => section = line,
            _ if section == PLATFORM_SECTION => {
                platform.push_str(line);
                platform.push('\n');
            }
            _ if section == INSTALLED_SECTION => {
                installed.push_str(line);
                installed.push('\n');
            }
            _ if section == CONSUMABLE_SECTION => {
                consumable.push_str(line);
                consumable.push('\n');
            }
            _ => {}
        }
    }
    Ok(ShellState {
        target: platform_target_from_uname(&platform)?,
        installed: parse_installed(&installed),
        consumable: parse_consumable(&consumable),
    })
}

fn parse_project_collection_overrides(
    output: &str,
    candidates: &BTreeSet<(String, String)>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut overrides = BTreeMap::<String, BTreeSet<String>>::new();
    for line in output.lines() {
        let Some((name, destination)) = line.split_once('\t') else {
            continue;
        };
        if !candidates.contains(&(name.to_string(), destination.to_string())) {
            continue;
        }
        overrides
            .entry(name.to_string())
            .or_default()
            .insert(destination.to_string());
    }
    overrides
}

#[cfg(test)]
mod tests;
