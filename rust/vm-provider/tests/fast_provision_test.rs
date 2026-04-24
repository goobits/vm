//! Regression tests for the canonical `.zshrc` template.

use std::collections::HashMap;

fn render_template_placeholders(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();

    for (key, value) in vars {
        result = result.replace(key, value);
    }
    result
}

#[test]
fn test_zshrc_template_exists() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    assert!(!template.is_empty(), "Zshrc template should be embedded");
    assert!(
        template.contains("Development Environment Shell Configuration"),
        "Template should contain the main header"
    );
}

#[test]
fn test_zshrc_template_has_key_sections() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    let key_sections = [
        "NVM Configuration",
        "Shell History",
        "Custom Prompt",
        "Terminal Color Configuration",
        "ZSH Syntax Highlighting",
    ];

    for section in &key_sections {
        assert!(
            template.contains(section),
            "Template should contain section: {}",
            section
        );
    }
}

#[test]
fn test_zshrc_template_has_expected_jinja_variables() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    let expected_variables = [
        "{{ project_name }}",
        "{{ terminal_emoji }}",
        "{{ terminal_username }}",
        "{{ project_path }}",
    ];

    for variable in &expected_variables {
        assert!(
            template.contains(variable),
            "Template should contain Jinja variable: {}",
            variable
        );
    }
}

#[test]
fn test_zshrc_template_render_placeholder_substitution() {
    let template = vm_provider::ZSHRC_TEMPLATE;

    let mut vars = HashMap::new();
    vars.insert("{{ project_name }}", "test-project");
    vars.insert("{{ terminal_emoji }}", "🚀");
    vars.insert("{{ terminal_username }}", "developer");
    vars.insert("{{ project_path }}", "/workspace");

    let result = render_template_placeholders(template, &vars);

    assert!(result.contains("test-project"));
    assert!(result.contains("🚀"));
    assert!(result.contains("developer"));
    assert!(result.contains("/workspace"));
}

#[test]
fn test_zshrc_template_has_git_branch_function() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    assert!(
        template.contains("function git_branch_name()"),
        "Template should define the git branch helper"
    );
}

#[test]
fn test_zshrc_template_has_project_alias_loop() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    assert!(
        template.contains("{% for alias in project_aliases %}"),
        "Template should render project-specific aliases from config"
    );
}

#[test]
fn test_zshrc_template_avoids_optional_service_dereferences() {
    let jinja_template = vm_provider::ZSHRC_TEMPLATE;

    let unsafe_paths = [
        "project_config.services.redis.port",
        "project_config.services.mongodb.port",
        "project_config.services.mysql.port",
        "project_config.services.headless_browser.display",
        "project_config.services.headless_browser.executable_path",
    ];

    for path in &unsafe_paths {
        assert!(
            !jinja_template.contains(path),
            "Jinja template should not directly dereference optional service path: {}",
            path
        );
    }
}
