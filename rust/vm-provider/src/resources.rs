// Embedded resources for VM provisioning
// These are compiled into the binary for portability

pub const ANSIBLE_PLAYBOOK: &str = include_str!("resources/ansible/playbook.yml");
pub const MANAGE_SERVICE_TASK: &str = include_str!("resources/ansible/tasks/manage-service.yml");
pub const SERVICE_DEFINITIONS: &str = include_str!("resources/services/service_definitions.yml");
pub const ZSHRC_TEMPLATE: &str = include_str!("resources/templates/zshrc.j2");
pub const THEMES_JSON: &str = include_str!("resources/templates/themes.json");
pub const CLAUDE_SETTINGS: &str = include_str!("resources/settings/claude-settings.json");
pub const GEMINI_SETTINGS: &str = include_str!("resources/settings/gemini-settings.json");