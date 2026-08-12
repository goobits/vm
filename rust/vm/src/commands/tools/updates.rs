use std::collections::BTreeMap;

use semver::Version;
use vm_config::config::{ToolUpdatePolicy, VmConfig};
use vm_packages::ToolArtifactRecord;

use super::guest::InstalledTool;

#[derive(Debug, Default)]
pub(super) struct UpdatePlan {
    pub(super) automatic: Vec<ToolArtifactRecord>,
    pub(super) prompt: Vec<ToolArtifactRecord>,
}

impl UpdatePlan {
    pub(super) fn automatic(self) -> Vec<ToolArtifactRecord> {
        self.automatic
    }

    /// All changes allowed by an explicit update command. `Off` updates never
    /// enter either collection, while required installs and pin repairs do.
    pub(super) fn eligible(mut self) -> Vec<ToolArtifactRecord> {
        self.automatic.append(&mut self.prompt);
        self.automatic
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
        let change = artifact.clone();

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

    #[test]
    fn automatic_selection_never_includes_prompt_updates() {
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

        let selected = plan(
            &config(ToolUpdatePolicy::Prompt, false),
            &available,
            &installed,
            &BTreeMap::from([("codex".into(), true)]),
        )
        .automatic();

        assert!(selected.is_empty());
    }

    #[test]
    fn explicit_update_selects_prompt_changes_but_respects_off() {
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
        let consumable = BTreeMap::from([("codex".into(), true)]);

        let selected = plan(
            &config(ToolUpdatePolicy::Prompt, false),
            &available,
            &installed,
            &consumable,
        )
        .eligible();
        let disabled = plan(
            &config(ToolUpdatePolicy::Off, false),
            &available,
            &installed,
            &consumable,
        )
        .eligible();

        assert_eq!(selected.len(), 1);
        assert!(disabled.is_empty());
    }
}
