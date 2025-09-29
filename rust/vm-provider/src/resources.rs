// Embedded resources for VM provisioning
// These are compiled into the binary for portability

use std::fs;
use std::path::Path;
use vm_core::error::Result;

pub const ANSIBLE_PLAYBOOK: &str = include_str!("resources/ansible/playbook.yml");
pub const MANAGE_SERVICE_TASK: &str = include_str!("resources/ansible/tasks/manage-service.yml");
pub const SERVICE_DEFINITIONS: &str = include_str!("resources/services/service_definitions.yml");
pub const ZSHRC_TEMPLATE: &str = include_str!("resources/templates/zshrc.j2");
pub const THEMES_JSON: &str = include_str!("resources/templates/themes.json");
pub const CLAUDE_SETTINGS: &str = include_str!("resources/settings/claude-settings.json");
pub const GEMINI_SETTINGS: &str = include_str!("resources/settings/gemini-settings.json");

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
        shared_dir.join("gemini-settings"),
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
            directories[2].join("service_definitions.yml"),
            SERVICE_DEFINITIONS,
        ),
        (directories[3].join("zshrc.j2"), ZSHRC_TEMPLATE),
        (shared_dir.join("themes.json"), THEMES_JSON),
        (directories[5].join("settings.json"), CLAUDE_SETTINGS),
        (directories[6].join("settings.json"), GEMINI_SETTINGS),
    ];

    file_operations[..]
        .par_iter()
        .try_for_each(|(path, content)| fs::write(path, content))?;

    Ok(())
}
