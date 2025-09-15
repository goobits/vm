// Embedded resources for VM provisioning
// These are compiled into the binary for portability

use std::fs;
use std::path::Path;
use anyhow::Result;

pub const ANSIBLE_PLAYBOOK: &str = include_str!("resources/ansible/playbook.yml");
pub const MANAGE_SERVICE_TASK: &str = include_str!("resources/ansible/tasks/manage-service.yml");
pub const SERVICE_DEFINITIONS: &str = include_str!("resources/services/service_definitions.yml");
pub const ZSHRC_TEMPLATE: &str = include_str!("resources/templates/zshrc.j2");
pub const THEMES_JSON: &str = include_str!("resources/templates/themes.json");
pub const CLAUDE_SETTINGS: &str = include_str!("resources/settings/claude-settings.json");
pub const GEMINI_SETTINGS: &str = include_str!("resources/settings/gemini-settings.json");

/// Copy all embedded resources to the specified directory
pub fn copy_embedded_resources(shared_dir: &Path) -> Result<()> {
    // Create directory structure
    let ansible_dir = shared_dir.join("ansible");
    let tasks_dir = ansible_dir.join("tasks");
    let services_dir = shared_dir.join("services");
    let templates_dir = shared_dir.join("templates");
    let settings_dir = shared_dir.join("settings");

    fs::create_dir_all(&ansible_dir)?;
    fs::create_dir_all(&tasks_dir)?;
    fs::create_dir_all(&services_dir)?;
    fs::create_dir_all(&templates_dir)?;
    fs::create_dir_all(&settings_dir)?;

    // Write embedded resources to files
    fs::write(ansible_dir.join("playbook.yml"), ANSIBLE_PLAYBOOK)?;
    fs::write(tasks_dir.join("manage-service.yml"), MANAGE_SERVICE_TASK)?;
    fs::write(services_dir.join("service_definitions.yml"), SERVICE_DEFINITIONS)?;
    fs::write(templates_dir.join("zshrc.j2"), ZSHRC_TEMPLATE)?;
    fs::write(shared_dir.join("themes.json"), THEMES_JSON)?;

    // Create claude and gemini settings directories
    let claude_settings_dir = shared_dir.join("claude-settings");
    let gemini_settings_dir = shared_dir.join("gemini-settings");
    fs::create_dir_all(&claude_settings_dir)?;
    fs::create_dir_all(&gemini_settings_dir)?;

    fs::write(claude_settings_dir.join("settings.json"), CLAUDE_SETTINGS)?;
    fs::write(gemini_settings_dir.join("settings.json"), GEMINI_SETTINGS)?;

    Ok(())
}