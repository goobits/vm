use std::collections::BTreeMap;
use std::time::Duration;

use futures::future::join_all;
use serde::Serialize;
use vm_config::config::VmConfig;
use vm_packages::{
    validate_tool_name, validate_tool_target, validate_tool_version, PackageInfrastructureClient,
    ToolArtifactRecord, ToolIndex,
};

use crate::error::{VmError, VmResult};

use super::appliance::configured_state_and_client;
use super::files::ApplianceFiles;
use super::runtime::gateway_for_provider;

const CACHE_MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);
const TOOL_TARGETS: [&str; 3] = ["linux-arm64", "linux-amd64", "darwin-arm64"];

#[derive(Debug, Clone)]
pub(in crate::commands) struct CachedToolCatalog {
    pub(in crate::commands) artifacts: BTreeMap<String, ToolArtifactRecord>,
    pub(in crate::commands) missing: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::commands) enum RefreshOutcome {
    Refreshed,
    AlreadyRunning,
}

pub(in crate::commands) async fn refresh(config: &VmConfig) -> VmResult<RefreshOutcome> {
    let files = ApplianceFiles::discover()?;
    let Some(_lock) = files.acquire_tool_cache_lock()? else {
        return Ok(RefreshOutcome::AlreadyRunning);
    };
    let (_, client) = configured_state_and_client(&files)?;

    let indexes = join_all(TOOL_TARGETS.into_iter().map(|target| {
        let client = client.clone();
        async move { (target, client.tool_index(target).await) }
    }))
    .await;
    let mut refreshed = 0_usize;
    let mut last_error = None;
    for (target, result) in indexes {
        match result {
            Ok(index) => {
                write_json(&files, &index_cache_name(target), &index)?;
                refreshed += 1;
            }
            Err(error) => last_error = Some(error),
        }
    }
    if refreshed == 0 {
        return Err(last_error.map_or_else(
            || VmError::validation("No tool indexes were available", None::<String>),
            VmError::from,
        ));
    }

    let pins = config
        .tools
        .entries
        .iter()
        .filter_map(|(name, tool)| {
            tool.version
                .as_deref()
                .filter(|version| *version != "latest")
                .map(|version| (name.clone(), version.to_string()))
        })
        .flat_map(|(name, version)| {
            TOOL_TARGETS
                .into_iter()
                .map(move |target| (name.clone(), version.clone(), target))
        });
    let resolutions = join_all(pins.map(|(name, version, target)| {
        let client = client.clone();
        async move {
            let result = client.resolve_tool(&name, Some(&version), target).await;
            (name, version, target, result)
        }
    }))
    .await;
    for (name, version, target, result) in resolutions {
        if let Ok(artifact) = result {
            write_json(
                &files,
                &resolution_cache_name(&name, &version, target)?,
                &artifact,
            )?;
        }
    }
    Ok(RefreshOutcome::Refreshed)
}

pub(in crate::commands) fn cached(
    config: &VmConfig,
    target: &str,
) -> VmResult<Option<CachedToolCatalog>> {
    let files = ApplianceFiles::discover()?;
    cached_from(&files, config, target)
}

pub(in crate::commands) fn has_fresh_catalog() -> bool {
    ApplianceFiles::discover().is_ok_and(|files| {
        TOOL_TARGETS.iter().any(|target| {
            files
                .read_tool_cache(&index_cache_name(target), CACHE_MAX_AGE)
                .is_ok_and(|content| content.is_some())
        })
    })
}

fn cached_from(
    files: &ApplianceFiles,
    config: &VmConfig,
    target: &str,
) -> VmResult<Option<CachedToolCatalog>> {
    validate_tool_target(target).map_err(VmError::from)?;
    let Some(index) = read_json::<ToolIndex>(files, &index_cache_name(target), CACHE_MAX_AGE)?
    else {
        return Ok(None);
    };
    if index.target != target {
        return Ok(None);
    }

    let mut artifacts = BTreeMap::new();
    let mut missing = Vec::new();
    for (name, selection) in &config.tools.entries {
        let artifact = if selection.tracks_latest() {
            index.tools.get(name).cloned()
        } else {
            let version = selection.version.as_deref().expect("pinned version exists");
            read_json::<ToolArtifactRecord>(
                files,
                &resolution_cache_name(name, version, target)?,
                CACHE_MAX_AGE,
            )?
            .or_else(|| {
                index
                    .tools
                    .get(name)
                    .filter(|artifact| artifact.version == version)
                    .cloned()
            })
        };
        match artifact {
            Some(artifact) => {
                validate_cached_artifact(name, target, &artifact)?;
                artifacts.insert(name.clone(), artifact);
            }
            None => missing.push(name.clone()),
        }
    }
    Ok(Some(CachedToolCatalog { artifacts, missing }))
}

fn validate_cached_artifact(
    configured_name: &str,
    requested_target: &str,
    artifact: &ToolArtifactRecord,
) -> VmResult<()> {
    artifact.validate().map_err(VmError::from)?;
    if artifact.tool != configured_name {
        return Err(VmError::validation(
            "Tool catalog key does not match its artifact",
            None::<String>,
        ));
    }
    if artifact.target != requested_target && artifact.target != "any" {
        return Err(VmError::validation(
            format!(
                "Tool artifact target '{}' cannot be used by '{requested_target}'",
                artifact.target
            ),
            None::<String>,
        ));
    }
    Ok(())
}

pub(in crate::commands) fn gateway(provider: &str) -> VmResult<String> {
    let files = ApplianceFiles::discover()?;
    let (state, _) = configured_state_and_client(&files)?;
    gateway_for_provider(&state, provider)
}

pub(in crate::commands) fn read_token() -> VmResult<String> {
    ApplianceFiles::discover()?.read_token()
}

pub(in crate::commands) fn client() -> VmResult<PackageInfrastructureClient> {
    let files = ApplianceFiles::discover()?;
    super::appliance::configured_client(&files)
}

fn index_cache_name(target: &str) -> String {
    format!("index-{target}.json")
}

fn resolution_cache_name(name: &str, version: &str, target: &str) -> VmResult<String> {
    validate_tool_name(name).map_err(VmError::from)?;
    validate_tool_version(version).map_err(VmError::from)?;
    validate_tool_target(target).map_err(VmError::from)?;
    Ok(format!("resolution-{name}-{version}-{target}.json"))
}

fn read_json<T: serde::de::DeserializeOwned>(
    files: &ApplianceFiles,
    name: &str,
    max_age: Duration,
) -> VmResult<Option<T>> {
    files
        .read_tool_cache(name, max_age)?
        .map(|content| serde_json::from_slice(&content).map_err(VmError::from))
        .transpose()
}

fn write_json(files: &ApplianceFiles, name: &str, value: &impl Serialize) -> VmResult<()> {
    let mut content = serde_json::to_vec_pretty(value).map_err(VmError::from)?;
    content.push(b'\n');
    files.write_tool_cache(name, &content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vm_config::config::{ToolConfig, ToolUpdatePolicy, ToolsConfig};
    use vm_packages::ToolKind;

    #[test]
    fn resolution_cache_names_are_flat_and_validated() {
        assert_eq!(
            resolution_cache_name("agent-skills", "1.2.3", "linux-arm64").unwrap(),
            "resolution-agent-skills-1.2.3-linux-arm64.json"
        );
        assert!(resolution_cache_name("../skills", "1.2.3", "linux-arm64").is_err());
    }

    #[test]
    fn cached_catalog_keeps_one_atomic_record_per_selected_tool() {
        let directory = tempfile::tempdir().unwrap();
        let files = ApplianceFiles::at(directory.path().join("packages"));
        let artifact = ToolArtifactRecord {
            tool: "agent-skills".into(),
            kind: ToolKind::Collection,
            version: "1.0.0".into(),
            target: "any".into(),
            artifact_digest: "a".repeat(64),
            size_bytes: 1,
            links: BTreeMap::from([(".codex/skills".into(), "skills".into())]),
            source_repository: "https://example.com/skills.git".into(),
            source_commit: "b".repeat(40),
            tag: "v1.0.0".into(),
            artifact_path: vm_packages::tool_artifact_path(
                "agent-skills",
                "1.0.0",
                "any",
                &"a".repeat(64),
            ),
            actor: "release".into(),
            published_at: Utc::now(),
            receipt_id: "receipt-1".into(),
        };
        write_json(
            &files,
            &index_cache_name("linux-arm64"),
            &ToolIndex {
                target: "linux-arm64".into(),
                generated_at: Utc::now(),
                tools: BTreeMap::from([("agent-skills".into(), artifact.clone())]),
            },
        )
        .unwrap();
        let config = VmConfig {
            tools: ToolsConfig {
                updates: ToolUpdatePolicy::Prompt,
                entries: BTreeMap::from([("agent-skills".into(), ToolConfig::default())])
                    .into_iter()
                    .collect(),
            },
            ..Default::default()
        };

        let catalog = cached_from(&files, &config, "linux-arm64")
            .unwrap()
            .unwrap();
        assert_eq!(catalog.artifacts["agent-skills"], artifact);
    }
}
