use vm_config::config::VmConfig;
use vm_core::vm_success;
use vm_packages::{RegisterTool, ToolKind};

use crate::error::{VmError, VmResult};

use super::super::packages::{self, tooling};

struct BuiltinTool {
    name: &'static str,
    kind: ToolKind,
    repository: &'static str,
    branch: &'static str,
    requires_git_auth: bool,
}

const BUILTIN_TOOLS: &[BuiltinTool] = &[BuiltinTool {
    name: "agent-skills",
    kind: ToolKind::Collection,
    repository: "ssh://git@github.com/goobits/agent-skills.git",
    branch: "main",
    requires_git_auth: true,
}];

pub(super) async fn prepare(configs: &[VmConfig]) -> VmResult<()> {
    if configs.iter().all(|config| config.tools.entries.is_empty()) {
        return Ok(());
    }
    ensure_builtin_releases(configs).await?;
    tooling::refresh_many(configs).await?;
    Ok(())
}

async fn ensure_builtin_releases(configs: &[VmConfig]) -> VmResult<()> {
    let selected = BUILTIN_TOOLS
        .iter()
        .filter(|tool| {
            configs
                .iter()
                .any(|config| config.tools.entries.contains_key(tool.name))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(());
    }
    let client = tooling::client()?;
    let definitions = client.tools().await?;
    for tool in selected {
        if !definitions
            .iter()
            .any(|definition| definition.name == tool.name)
        {
            client
                .register_tool(&RegisterTool {
                    name: tool.name.into(),
                    kind: tool.kind,
                    repository: tool.repository.into(),
                    default_branch: tool.branch.into(),
                    build_sources: Vec::new(),
                    workspace_release: false,
                })
                .await?;
            vm_success!("Registered built-in tool '{}'", tool.name);
        }

        let inventory = client.tool(tool.name).await?;
        let has_release = configs
            .iter()
            .filter_map(|config| config.tools.entries.get(tool.name))
            .all(|selection| match selection.version.as_deref() {
                None | Some("latest") => !inventory.artifacts.is_empty(),
                Some(version) => inventory
                    .artifacts
                    .iter()
                    .any(|artifact| artifact.version == version),
            });
        if has_release {
            continue;
        }
        if tool.requires_git_auth
            && inventory.definition.repository == tool.repository
            && !packages::git_auth_configured()?
        {
            return Err(VmError::validation(
                format!("Built-in tool '{}' requires private Git access", tool.name),
                Some(format!(
                    "Configure controller Git access, then create and release a managed '{}' checkout from a writable environment",
                    tool.name
                )),
            ));
        }
        return Err(VmError::validation(
            format!("Built-in tool '{}' is registered but not published", tool.name),
            Some(format!(
                "Create and release a managed '{}' checkout from a writable environment, then retry `vm tools update`",
                tool.name
            )),
        ));
    }
    Ok(())
}
