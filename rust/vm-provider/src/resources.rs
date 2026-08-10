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
#[cfg(any(feature = "tart", test))]
pub(crate) const SHELL_CONFIG_VERSION: &str = "4";
pub const THEMES_JSON: &str = include_str!("resources/templates/themes.json");
pub const CLAUDE_SETTINGS_TEMPLATE: &str =
    include_str!("resources/settings/claude-settings.json.j2");
pub const AI_TOOLS_INSTALLER: &str = include_str!("resources/scripts/install-ai-tools.sh");
pub(crate) const HOME_STATE_REPAIR: &str = include_str!("resources/scripts/repair-home-state.sh");

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
        (
            directories[6].join("repair-home-state.sh"),
            HOME_STATE_REPAIR,
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
    use super::{
        AI_TOOLS_INSTALLER, ANSIBLE_PLAYBOOK, HOME_STATE_REPAIR, SHELL_CONFIG_VERSION,
        ZSHRC_TEMPLATE,
    };

    #[test]
    fn ai_tools_use_one_current_runtime_installer() {
        assert!(AI_TOOLS_INSTALLER.contains("https://antigravity.google/cli/install.sh"));
        assert!(AI_TOOLS_INSTALLER.contains("https://claude.ai/install.sh"));
        assert!(AI_TOOLS_INSTALLER.contains("https://chatgpt.com/codex/install.sh"));
        assert!(!AI_TOOLS_INSTALLER.contains("npm install -g"));
        assert!(AI_TOOLS_INSTALLER.contains("INSTALLER_STATE_VERSION=1"));
        assert!(AI_TOOLS_INSTALLER.contains("VM_AI_TOOLS_FORCE"));
        assert!(AI_TOOLS_INSTALLER.contains("VM_AI_TOOL_CURRENT="));
        assert!(AI_TOOLS_INSTALLER.contains("VM_AI_TOOL_CHANGED="));
        assert!(AI_TOOLS_INSTALLER.contains("shell_arg=stable"));
        assert_eq!(
            AI_TOOLS_INSTALLER
                .matches("refresh_scope=automatic")
                .count(),
            2
        );
        assert!(AI_TOOLS_INSTALLER.contains("refresh_scope=\"$refresh_key\""));
        assert_eq!(ANSIBLE_PLAYBOOK.matches("install-ai-tools.sh").count(), 1);
        assert!(!ANSIBLE_PLAYBOOK.contains("@google/gemini-cli@latest"));
        assert!(ANSIBLE_PLAYBOOK
            .contains("changed_when: \"'VM_AI_TOOL_CHANGED=' in ai_tools_install.stdout\""));
    }

    #[test]
    fn ai_tool_path_precedes_shell_wrapper_detection() {
        let path = ZSHRC_TEMPLATE
            .find("export PATH=\"$HOME/.local/bin:$PATH\"")
            .unwrap();

        assert!(path < ZSHRC_TEMPLATE.find("if command -v claude").unwrap());
        assert!(path < ZSHRC_TEMPLATE.find("if command -v codex").unwrap());
        assert_eq!(
            ZSHRC_TEMPLATE
                .matches("export PATH=\"$HOME/.local/bin:$PATH\"")
                .count(),
            1
        );
        assert!(ZSHRC_TEMPLATE.contains(&format!("VM_SHELL_CONFIG_VERSION={SHELL_CONFIG_VERSION}")));
    }

    #[test]
    fn home_repair_is_versioned_and_mount_aware() {
        assert!(HOME_STATE_REPAIR.contains("REPAIR_VERSION=1"));
        assert!(HOME_STATE_REPAIR.contains("home-repair"));
        assert!(HOME_STATE_REPAIR.contains("VM_HOME_REPAIR_FORCE"));
        assert!(HOME_STATE_REPAIR.contains("full_repair"));
        assert!(HOME_STATE_REPAIR.contains("state_fingerprint"));
        assert!(HOME_STATE_REPAIR.contains("find \"$path\" -xdev"));
        assert_eq!(ANSIBLE_PLAYBOOK.matches("repair-home-state.sh").count(), 0);
    }
}
