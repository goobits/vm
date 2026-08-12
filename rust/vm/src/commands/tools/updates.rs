use std::collections::BTreeMap;

use semver::Version;
use vm_config::config::{ToolUpdatePolicy, VmConfig};
use vm_packages::ToolArtifactRecord;

use crate::error::{VmError, VmResult};

use super::guest::InstalledTool;

#[derive(Debug, Clone)]
pub(super) struct ToolChange {
    pub(super) artifact: ToolArtifactRecord,
    pub(super) current_version: Option<String>,
}

impl ToolChange {
    pub(super) fn label(&self) -> String {
        self.current_version.as_ref().map_or_else(
            || format!("{} {} (install)", self.artifact.tool, self.artifact.version),
            |current| {
                format!(
                    "{} {} → {}",
                    self.artifact.tool, current, self.artifact.version
                )
            },
        )
    }
}

#[derive(Debug, Default)]
pub(super) struct UpdatePlan {
    pub(super) automatic: Vec<ToolChange>,
    pub(super) prompt: Vec<ToolChange>,
}

impl UpdatePlan {
    pub(super) fn selected(self, all: bool) -> VmResult<Vec<ToolArtifactRecord>> {
        let mut selected = self
            .automatic
            .into_iter()
            .map(|change| change.artifact)
            .collect::<Vec<_>>();
        if all {
            selected.extend(self.prompt.into_iter().map(|change| change.artifact));
            return Ok(selected);
        }
        let labels = self
            .prompt
            .iter()
            .map(ToolChange::label)
            .collect::<Vec<_>>();
        let defaults = vec![true; labels.len()];
        let indexes = vm_core::prompts::multi_select("Tool updates", &labels, &defaults)
            .map_err(|error| VmError::general(error, "Could not read the tool update checklist"))?;
        selected.extend(
            indexes
                .into_iter()
                .filter_map(|index| self.prompt.get(index))
                .map(|change| change.artifact.clone()),
        );
        Ok(selected)
    }
}

pub(super) fn plan(
    config: &VmConfig,
    available: &BTreeMap<String, ToolArtifactRecord>,
    installed: &BTreeMap<String, InstalledTool>,
    consumable: &BTreeMap<String, bool>,
) -> UpdatePlan {
    let mut plan = UpdatePlan::default();
    for (name, artifact) in available {
        let current = installed
            .get(name)
            .filter(|_| consumable.get(name).copied().unwrap_or(false));
        if current.is_some_and(|current| current.digest == artifact.artifact_digest) {
            continue;
        }
        let selection = &config.tools.entries[name];
        let change = ToolChange {
            artifact: artifact.clone(),
            current_version: current.map(|current| current.version.clone()),
        };

        let is_install = current.is_none();
        let is_pin_reconciliation = !selection.tracks_latest();
        let is_newer = current.map_or(true, |current| {
            match (
                Version::parse(&current.version),
                Version::parse(&artifact.version),
            ) {
                (Ok(current), Ok(available)) => available > current,
                _ => true,
            }
        });
        if !is_install && !is_pin_reconciliation && !is_newer {
            continue;
        }

        if is_install || is_pin_reconciliation {
            plan.automatic.push(change);
            continue;
        }
        match selection.effective_updates(config.tools.updates) {
            ToolUpdatePolicy::Auto => plan.automatic.push(change),
            ToolUpdatePolicy::Prompt => plan.prompt.push(change),
            ToolUpdatePolicy::Off => {}
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use vm_config::config::{ToolConfig, ToolsConfig};
    use vm_packages::ToolKind;

    fn artifact(name: &str, version: &str, digest: char) -> ToolArtifactRecord {
        ToolArtifactRecord {
            tool: name.into(),
            kind: ToolKind::Binary,
            version: version.into(),
            target: "linux-arm64".into(),
            artifact_digest: digest.to_string().repeat(64),
            size_bytes: 1,
            links: BTreeMap::from([(".local/bin/tool".into(), "bin/tool".into())]),
            source_repository: "https://example.com/tool.git".into(),
            source_commit: "f".repeat(40),
            tag: format!("v{version}"),
            artifact_path: "/tools/artifacts/tool".into(),
            actor: "release".into(),
            published_at: Utc::now(),
            receipt_id: "receipt".into(),
        }
    }

    fn config(policy: ToolUpdatePolicy, pinned: bool) -> VmConfig {
        let mut tools = ToolsConfig {
            updates: policy,
            ..Default::default()
        };
        tools.entries.insert(
            "codex".into(),
            ToolConfig {
                version: pinned.then(|| "2.0.0".into()),
                updates: None,
            },
        );
        VmConfig {
            tools,
            ..Default::default()
        }
    }

    #[test]
    fn installs_and_pin_changes_are_automatic_but_latest_updates_follow_policy() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "2.0.0", 'b'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);

        let prompted = plan(
            &config(ToolUpdatePolicy::Prompt, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        );
        assert_eq!(prompted.prompt.len(), 1);
        let automatic = plan(
            &config(ToolUpdatePolicy::Off, true),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        );
        assert_eq!(automatic.automatic.len(), 1);
        let initial = plan(
            &config(ToolUpdatePolicy::Off, false),
            &available,
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert_eq!(initial.automatic.len(), 1);
    }

    #[test]
    fn latest_never_downgrades_from_a_newer_guest_version() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "1.0.0", 'b'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "2.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);
        let plan = plan(
            &config(ToolUpdatePolicy::Auto, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        );
        assert!(plan.automatic.is_empty());
        assert!(plan.prompt.is_empty());
    }

    #[test]
    fn matching_but_non_consumable_release_is_reinstalled() {
        let available = BTreeMap::from([("codex".into(), artifact("codex", "1.0.0", 'a'))]);
        let installed = BTreeMap::from([(
            "codex".into(),
            InstalledTool {
                name: "codex".into(),
                version: "1.0.0".into(),
                target: "linux-arm64".into(),
                digest: "a".repeat(64),
            },
        )]);

        let plan = plan(
            &config(ToolUpdatePolicy::Off, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), false)]),
        );

        assert_eq!(plan.automatic.len(), 1);
        assert!(plan.prompt.is_empty());
    }
}
