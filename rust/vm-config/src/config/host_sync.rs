use serde::{Deserialize, Serialize};

/// Host-to-guest synchronization policy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostSyncConfig {
    #[serde(default = "default_true")]
    pub git_config: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ssh_agent: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ssh_config: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dotfiles: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ai_tools: Option<AiSyncConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktrees: Option<WorktreesConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AiSyncConfig {
    Boolean(bool),
    Detailed(AiSyncTools),
}

impl Default for AiSyncConfig {
    fn default() -> Self {
        Self::Detailed(AiSyncTools::default())
    }
}

impl AiSyncConfig {
    pub fn is_claude_enabled(&self) -> bool {
        match self {
            Self::Boolean(enabled) => *enabled,
            Self::Detailed(tools) => tools.claude,
        }
    }

    pub fn is_antigravity_enabled(&self) -> bool {
        match self {
            Self::Boolean(enabled) => *enabled,
            Self::Detailed(tools) => tools.antigravity,
        }
    }

    pub fn is_codex_enabled(&self) -> bool {
        match self {
            Self::Boolean(enabled) => *enabled,
            Self::Detailed(tools) => tools.codex,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiSyncTools {
    #[serde(default = "default_true")]
    pub claude: bool,
    #[serde(default = "default_true")]
    pub antigravity: bool,
    #[serde(default)]
    pub codex: bool,
}

impl Default for AiSyncTools {
    fn default() -> Self {
        Self {
            claude: true,
            antigravity: true,
            codex: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreesConfig {
    #[serde(default = "default_worktrees_enabled")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_path: Option<String>,
}

impl Default for WorktreesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_path: None,
        }
    }
}

fn is_false(value: &bool) -> bool {
    !value
}

fn default_true() -> bool {
    true
}

fn default_worktrees_enabled() -> bool {
    true
}
