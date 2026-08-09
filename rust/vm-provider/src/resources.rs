// Embedded resources for VM provisioning
// These are compiled into the binary for portability

use std::fs;
use std::path::Path;
use vm_core::error::Result;

pub const ANSIBLE_PLAYBOOK: &str = include_str!("resources/ansible/playbook.yml");
pub const MANAGE_SERVICE_TASK: &str = include_str!("resources/ansible/tasks/manage-service.yml");
pub const BOOTSTRAP_NODE_TASK: &str = include_str!("resources/ansible/tasks/bootstrap-node.yml");
pub const NODE_TOOLCHAIN_TASK: &str = include_str!("resources/ansible/tasks/node-toolchain.yml");
pub const SERVICE_DEFINITIONS: &str = include_str!("resources/services/service_definitions.yml");
pub const ZSHRC_TEMPLATE: &str = include_str!("resources/templates/zshrc.j2");
pub const THEMES_JSON: &str = include_str!("resources/templates/themes.json");
pub const CLAUDE_SETTINGS_TEMPLATE: &str =
    include_str!("resources/settings/claude-settings.json.j2");
pub const AI_TOOLS_INSTALLER: &str = include_str!("resources/scripts/install-ai-tools.sh");

/// Copy all embedded resources to the specified directory
pub fn copy_embedded_resources(shared_dir: &Path) -> Result<()> {
    use rayon::prelude::*;

    // Create directory structure in parallel
    let directories = [
        shared_dir.join("ansible"),
        shared_dir.join("ansible").join("tasks"),
        shared_dir.join("services"),
        shared_dir.join("templates"),
        shared_dir.join("settings"),
        shared_dir.join("claude-settings"),
        shared_dir.join("scripts"),
    ];

    directories[..]
        .par_iter()
        .try_for_each(fs::create_dir_all)?;

    // Write embedded resources in parallel
    let file_operations = [
        (directories[0].join("playbook.yml"), ANSIBLE_PLAYBOOK),
        (
            directories[1].join("manage-service.yml"),
            MANAGE_SERVICE_TASK,
        ),
        (
            directories[1].join("bootstrap-node.yml"),
            BOOTSTRAP_NODE_TASK,
        ),
        (
            directories[1].join("node-toolchain.yml"),
            NODE_TOOLCHAIN_TASK,
        ),
        (
            directories[2].join("service_definitions.yml"),
            SERVICE_DEFINITIONS,
        ),
        (directories[3].join("zshrc.j2"), ZSHRC_TEMPLATE),
        (shared_dir.join("themes.json"), THEMES_JSON),
        (
            directories[5].join("settings.json.j2"),
            CLAUDE_SETTINGS_TEMPLATE,
        ),
        (
            directories[6].join("install-ai-tools.sh"),
            AI_TOOLS_INSTALLER,
        ),
    ];

    file_operations[..]
        .par_iter()
        .try_for_each(|(path, content)| write_if_changed(path, content))?;

    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    match fs::read(path) {
        Ok(existing) if existing == content.as_bytes() => Ok(()),
        _ => fs::write(path, content).map_err(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::{AI_TOOLS_INSTALLER, ANSIBLE_PLAYBOOK};

    #[test]
    fn ai_tools_use_one_current_runtime_installer() {
        assert!(AI_TOOLS_INSTALLER.contains("https://antigravity.google/cli/install.sh"));
        assert!(AI_TOOLS_INSTALLER.contains("https://claude.ai/install.sh"));
        assert!(AI_TOOLS_INSTALLER.contains("@openai/codex@latest"));
        assert!(!AI_TOOLS_INSTALLER.contains("npm install -g @anthropic-ai/claude-code"));
        assert_eq!(ANSIBLE_PLAYBOOK.matches("install-ai-tools.sh").count(), 1);
        assert!(!ANSIBLE_PLAYBOOK.contains("@google/gemini-cli@latest"));
    }
}
