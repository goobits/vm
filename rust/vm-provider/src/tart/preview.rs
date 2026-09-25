//! Read-only Tart creation preview using the same source and run-option rules.

use serde::Serialize;
use vm_config::config::{MountAccess, VmConfig};
use vm_core::error::Result;

use super::{
    image::{configured_source, TartImageSource},
    provider::TartProvider,
    workspace::effective_sync_directory_for_config,
};
use crate::instance::extract_project_name;

#[derive(Serialize)]
struct TartPreview {
    provider: &'static str,
    name: String,
    source: Source,
    resources: Resources,
    run: Run,
    guest: Guest,
    shares: Vec<Share>,
    custom_storage: bool,
}

#[derive(Serialize)]
struct Source {
    kind: &'static str,
    reference: String,
}

#[derive(Serialize)]
struct Resources {
    #[serde(skip_serializing_if = "Option::is_none")]
    memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpus: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disk_gb: Option<u32>,
}

#[derive(Serialize)]
struct Run {
    arguments: Vec<String>,
}

#[derive(Serialize)]
struct Guest {
    os: &'static str,
    ssh_user: String,
    workspace: String,
}

#[derive(Serialize)]
struct Share {
    tag: String,
    host: &'static str,
    guest: String,
    access: &'static str,
}

pub fn render_tart_preview(config: &VmConfig, environment: Option<&str>) -> Result<String> {
    let name = match environment {
        Some(environment) => format!("{}-{environment}", extract_project_name(config)),
        None => extract_project_name(config).to_string(),
    };
    let source = match configured_source(config)? {
        TartImageSource::Image(reference) => Source {
            kind: "image",
            reference,
        },
        TartImageSource::Snapshot(reference) => Source {
            kind: "snapshot",
            reference,
        },
    };
    let resolved = TartProvider::resolved_tart_resources(config)?;
    let resources = Resources {
        memory_mb: resolved.memory_mb,
        cpus: resolved.cpus,
        disk_gb: config
            .tart
            .as_ref()
            .and_then(|tart| tart.disk_size.as_ref())
            .and_then(|size| size.to_gb()),
    };
    let workspace = effective_sync_directory_for_config(config);
    let mut shares = vec![Share {
        tag: "workspace".to_string(),
        host: "<host-path>",
        guest: workspace.clone(),
        access: config
            .project
            .as_ref()
            .map(|project| project.workspace_access.as_mode())
            .unwrap_or(MountAccess::ReadWrite.as_mode()),
    }];
    shares.extend(
        config
            .mounts
            .iter()
            .enumerate()
            .map(|(index, mount)| Share {
                tag: format!("vmmount{index}"),
                host: "<host-path>",
                guest: mount.target.to_string_lossy().into_owned(),
                access: mount.access.as_mode(),
            }),
    );
    if let Some(ai) = config
        .host_sync
        .as_ref()
        .and_then(|host_sync| host_sync.ai_tools.as_ref())
    {
        for (enabled, tag, guest) in [
            (ai.is_claude_enabled(), "claude-sync", "~/.claude"),
            (ai.is_antigravity_enabled(), "antigravity-sync", "~/.gemini"),
        ] {
            if enabled {
                shares.push(Share {
                    tag: tag.to_string(),
                    host: "<host-path>",
                    guest: guest.to_string(),
                    access: "rw",
                });
            }
        }
    }
    let directories = shares
        .iter()
        .map(|share| {
            format!(
                "{}:tag={}{}",
                share.host,
                share.tag,
                if share.access == "ro" { ":ro" } else { "" }
            )
        })
        .collect::<Vec<_>>();
    let preview = TartPreview {
        provider: "tart",
        name: name.clone(),
        source,
        resources,
        run: Run {
            arguments: run_args(config, &name, &directories),
        },
        guest: Guest {
            os: if TartProvider::is_macos_guest_config(config) {
                "macos"
            } else {
                "linux"
            },
            ssh_user: config
                .tart
                .as_ref()
                .and_then(|tart| tart.ssh_user.clone())
                .unwrap_or_else(|| "admin".into()),
            workspace,
        },
        shares,
        custom_storage: config
            .tart
            .as_ref()
            .and_then(|tart| tart.storage_path.as_ref())
            .is_some(),
    };
    serde_yaml_ng::to_string(&preview).map_err(Into::into)
}

pub(super) fn run_args(config: &VmConfig, name: &str, directories: &[String]) -> Vec<String> {
    let nested = config
        .tart
        .as_ref()
        .and_then(|tart| tart.nested)
        .unwrap_or(false)
        && !TartProvider::is_macos_guest_config(config);
    let mut args = vec![
        "tart".to_string(),
        "run".to_string(),
        "--no-graphics".to_string(),
    ];
    if nested {
        args.push("--nested".to_string());
    }
    for directory in directories {
        args.extend(["--dir".to_string(), directory.clone()]);
    }
    args.push(name.to_string());
    args
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::{ImageSpec, ProjectConfig, TartConfig, VmSettings};

    #[test]
    fn preview_redacts_host_paths_and_matches_tart_run_options() {
        let config = VmConfig {
            provider: Some("tart".into()),
            project: Some(ProjectConfig {
                name: Some("demo".into()),
                ..Default::default()
            }),
            vm: Some(VmSettings {
                image: Some(ImageSpec::String("vibe-tart-linux-base".into())),
                ..Default::default()
            }),
            tart: Some(TartConfig {
                guest_os: Some("linux".into()),
                nested: Some(true),
                storage_path: Some("/private/disk".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let rendered = render_tart_preview(&config, Some("test")).unwrap();
        assert!(rendered.contains("name: demo-test"));
        assert!(rendered.contains("--nested"));
        assert!(rendered.contains("<host-path>"));
        assert!(!rendered.contains("/private/disk"));
        assert!(rendered.contains("custom_storage: true"));
        assert!(rendered.contains("os: linux"));
    }

    #[test]
    fn explicit_macos_guest_omits_nested_run_flag() {
        let config = VmConfig {
            os: Some("macos".into()),
            tart: Some(TartConfig {
                nested: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(!run_args(&config, "demo", &[]).contains(&"--nested".to_string()));
    }
}
