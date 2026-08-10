use indexmap::IndexMap;
use semver::Version;
use serde::{Deserialize, Serialize};
use vm_core::error::{Result, VmError};

/// When an environment should apply newer tool releases.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolUpdatePolicy {
    #[default]
    Prompt,
    Auto,
    Off,
}

impl ToolUpdatePolicy {
    fn is_prompt(policy: &Self) -> bool {
        matches!(policy, Self::Prompt)
    }
}

/// Project-level tool selection and defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default, skip_serializing_if = "ToolUpdatePolicy::is_prompt")]
    pub updates: ToolUpdatePolicy,
    #[serde(flatten)]
    pub entries: IndexMap<String, ToolConfig>,
}

impl ToolsConfig {
    pub fn is_empty(config: &Self) -> bool {
        config.entries.is_empty() && ToolUpdatePolicy::is_prompt(&config.updates)
    }

    pub fn validate(&self) -> Result<()> {
        for (name, tool) in &self.entries {
            validate_tool_name(name)?;
            tool.validate(name)?;
        }
        Ok(())
    }
}

/// Per-tool overrides. An omitted version tracks the latest published release.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updates: Option<ToolUpdatePolicy>,
}

impl ToolConfig {
    pub fn tracks_latest(&self) -> bool {
        self.version
            .as_deref()
            .map_or(true, |version| version == "latest")
    }

    pub fn effective_updates(&self, defaults: ToolUpdatePolicy) -> ToolUpdatePolicy {
        self.updates.unwrap_or(defaults)
    }

    fn validate(&self, name: &str) -> Result<()> {
        if let Some(version) = self.version.as_deref() {
            if version != "latest" && Version::parse(version).is_err() {
                return Err(VmError::Config(format!(
                    "Invalid version '{version}' for tool '{name}': use a semantic version or 'latest'"
                )));
            }
        }
        Ok(())
    }
}

fn validate_tool_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid && name != "updates" {
        Ok(())
    } else {
        Err(VmError::Config(format!(
            "Invalid tool name '{name}': use letters, numbers, dashes, or underscores"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_level_tool_selection_and_overrides() {
        let tools: ToolsConfig = serde_yaml_ng::from_str(
            r#"
updates: prompt
codex: {}
agent-skills:
  version: 2.1.0
  updates: auto
"#,
        )
        .unwrap();

        assert_eq!(tools.entries.len(), 2);
        assert!(tools.entries["codex"].tracks_latest());
        assert_eq!(
            tools.entries["agent-skills"].version.as_deref(),
            Some("2.1.0")
        );
        assert_eq!(
            tools.entries["agent-skills"].effective_updates(tools.updates),
            ToolUpdatePolicy::Auto
        );
        tools.validate().unwrap();
    }

    #[test]
    fn rejects_invalid_names_versions_and_options() {
        let invalid_version: ToolsConfig =
            serde_yaml_ng::from_str("codex:\n  version: newest\n").unwrap();
        assert!(invalid_version.validate().is_err());

        let invalid_name: ToolsConfig = serde_yaml_ng::from_str("../codex: {}\n").unwrap();
        assert!(invalid_name.validate().is_err());

        assert!(serde_yaml_ng::from_str::<ToolsConfig>("codex:\n  typo: true\n").is_err());
    }
}
