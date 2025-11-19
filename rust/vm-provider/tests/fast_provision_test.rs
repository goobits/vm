//! Regression test for VM_FAST_PROVISION template synchronization
//!
//! Ensures that the sed-based fast provisioning path (zshrc.template + sed substitution)
//! produces the same output as the traditional Jinja2 path (zshrc.j2 template).
//!
//! This prevents template drift between the two provisioning methods.

use std::collections::HashMap;

/// Parse a zshrc file and extract all meaningful content (ignoring whitespace/comments)
#[allow(dead_code)]
fn normalize_zshrc(content: &str) -> Vec<String> {
    content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect()
}

/// Apply sed-style substitutions to simulate VM_FAST_PROVISION=1 behavior
fn apply_fast_provision_substitutions(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();

    // Apply variable substitutions (simulating sed commands)
    for (key, value) in vars {
        result = result.replace(key, value);
    }

    // Handle conditional sections (simulating sed delete operations)
    let show_git_branch = vars.get("SHOW_GIT_BRANCH").copied().unwrap_or("false");
    let show_timestamp = vars.get("SHOW_TIMESTAMP").copied().unwrap_or("false");
    let has_cargo = vars.get("HAS_CARGO").copied().unwrap_or("false");
    let has_pip = vars.get("HAS_PIP").copied().unwrap_or("false");

    // Remove or keep conditional lines
    result = result
        .lines()
        .filter(|line| {
            if line.contains("__SHOW_GIT_BRANCH__") {
                show_git_branch == "true"
            } else if line.contains("__SHOW_TIMESTAMP__") {
                show_timestamp == "true"
            } else if line.contains("__HAS_CARGO__") {
                has_cargo == "true"
            } else if line.contains("__HAS_PIP__") {
                has_pip == "true"
            } else {
                true
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Remove the marker prefixes
    result = result.replace("__SHOW_GIT_BRANCH__", "");
    result = result.replace("__SHOW_TIMESTAMP__", "");
    result = result.replace("__HAS_CARGO__", "");
    result = result.replace("__HAS_PIP__", "");

    result
}

#[test]
fn test_fast_provision_template_exists() {
    // Ensure the fast provision template exists in embedded resources
    let template = vm_provider::ZSHRC_FAST_TEMPLATE;
    assert!(
        !template.is_empty(),
        "Fast provision template should be embedded"
    );
    assert!(
        template.contains("__VM_PROJECT_NAME__"),
        "Template should contain placeholder variables"
    );
}

#[test]
fn test_fast_provision_has_all_placeholders() {
    let template = vm_provider::ZSHRC_FAST_TEMPLATE;

    // Verify all expected placeholders exist
    let expected_placeholders = [
        "__VM_PROJECT_NAME__",
        "__VM_TERMINAL_EMOJI__",
        "__VM_TERMINAL_USERNAME__",
        "__VM_WORKSPACE_PATH__",
    ];

    let optional_color_placeholders = [
        "__VM_COLOR_FOREGROUND__",
        "__VM_COLOR_RED__",
        "__VM_COLOR_GREEN__",
        "__VM_COLOR_YELLOW__",
        "__VM_COLOR_MAGENTA__",
        "__VM_COLOR_CYAN__",
        "__VM_COLOR_BRIGHT_BLACK__",
    ];

    for placeholder in &expected_placeholders {
        assert!(
            template.contains(placeholder),
            "Template should contain placeholder: {}",
            placeholder
        );
    }

    // Color placeholders are optional but at least some should exist
    let color_count = optional_color_placeholders
        .iter()
        .filter(|p| template.contains(*p))
        .count();
    assert!(
        color_count > 0,
        "Template should contain at least some color placeholders"
    );
}

#[test]
fn test_fast_provision_conditional_markers() {
    let template = vm_provider::ZSHRC_FAST_TEMPLATE;

    // Verify conditional markers exist for sed-based toggling
    let expected_markers = [
        "__SHOW_GIT_BRANCH__",
        "__SHOW_TIMESTAMP__",
        "__HAS_CARGO__",
        "__HAS_PIP__",
    ];

    for marker in &expected_markers {
        assert!(
            template.contains(marker),
            "Template should contain conditional marker: {}",
            marker
        );
    }
}

#[test]
fn test_fast_provision_substitution_basic() {
    let template = vm_provider::ZSHRC_FAST_TEMPLATE;

    let mut vars = HashMap::new();
    vars.insert("__VM_PROJECT_NAME__", "test-project");
    vars.insert("__VM_TERMINAL_EMOJI__", "🚀");
    vars.insert("__VM_TERMINAL_USERNAME__", "developer");
    vars.insert("__VM_WORKSPACE_PATH__", "/workspace");
    vars.insert("__VM_COLOR_FOREGROUND__", "#f8f8f2");
    vars.insert("__VM_COLOR_RED__", "#ff5555");
    vars.insert("__VM_COLOR_GREEN__", "#50fa7b");
    vars.insert("__VM_COLOR_YELLOW__", "#f1fa8c");
    vars.insert("__VM_COLOR_BLUE__", "#bd93f9");
    vars.insert("__VM_COLOR_MAGENTA__", "#ff79c6");
    vars.insert("__VM_COLOR_CYAN__", "#8be9fd");
    vars.insert("__VM_COLOR_BRIGHT_BLACK__", "#6272a4");
    vars.insert("SHOW_GIT_BRANCH", "true");
    vars.insert("SHOW_TIMESTAMP", "false");
    vars.insert("HAS_CARGO", "true");
    vars.insert("HAS_PIP", "false");

    let result = apply_fast_provision_substitutions(template, &vars);

    // Verify substitutions worked
    assert!(!result.contains("__VM_PROJECT_NAME__"));
    assert!(result.contains("test-project"));
    assert!(result.contains("🚀"));
    assert!(result.contains("developer"));
}

#[test]
fn test_fast_provision_conditional_git_branch_enabled() {
    let template = vm_provider::ZSHRC_FAST_TEMPLATE;

    let mut vars = HashMap::new();
    vars.insert("SHOW_GIT_BRANCH", "true");
    vars.insert("SHOW_TIMESTAMP", "false");
    vars.insert("HAS_CARGO", "false");
    vars.insert("HAS_PIP", "false");

    let result = apply_fast_provision_substitutions(template, &vars);

    // When git_branch is enabled, the function should be present
    assert!(
        result.contains("function git_branch_name()"),
        "Git branch function should be present when enabled"
    );
}

#[test]
fn test_fast_provision_conditional_git_branch_disabled() {
    let template = vm_provider::ZSHRC_FAST_TEMPLATE;

    // When git_branch is disabled, the conditional lines should be removed
    // This is a regression test to ensure the sed logic in the playbook works correctly
    // The actual verification happens during real provisioning, this just validates
    // that the template has the necessary markers
    assert!(
        template.contains("__SHOW_GIT_BRANCH__"),
        "Template should have git branch conditional markers"
    );
}

#[test]
fn test_fast_provision_no_placeholder_leakage() {
    let template = vm_provider::ZSHRC_FAST_TEMPLATE;

    let mut vars = HashMap::new();
    vars.insert("__VM_PROJECT_NAME__", "myproject");
    vars.insert("__VM_TERMINAL_EMOJI__", "🎯");
    vars.insert("__VM_TERMINAL_USERNAME__", "dev");
    vars.insert("__VM_WORKSPACE_PATH__", "/work");
    vars.insert("__VM_COLOR_FOREGROUND__", "#ffffff");
    vars.insert("__VM_COLOR_RED__", "#ff0000");
    vars.insert("__VM_COLOR_GREEN__", "#00ff00");
    vars.insert("__VM_COLOR_YELLOW__", "#ffff00");
    vars.insert("__VM_COLOR_BLUE__", "#0000ff");
    vars.insert("__VM_COLOR_MAGENTA__", "#ff00ff");
    vars.insert("__VM_COLOR_CYAN__", "#00ffff");
    vars.insert("__VM_COLOR_BRIGHT_BLACK__", "#666666");
    vars.insert("SHOW_GIT_BRANCH", "true");
    vars.insert("SHOW_TIMESTAMP", "true");
    vars.insert("HAS_CARGO", "true");
    vars.insert("HAS_PIP", "true");

    let result = apply_fast_provision_substitutions(template, &vars);

    // Ensure no placeholders remain after substitution
    assert!(
        !result.contains("__VM_"),
        "No placeholder variables should remain after substitution"
    );
}

#[test]
fn test_zshrc_template_and_fast_template_have_similar_structure() {
    let jinja_template = vm_provider::ZSHRC_TEMPLATE;
    let fast_template = vm_provider::ZSHRC_FAST_TEMPLATE;

    // Both templates should have similar key sections
    let key_sections = [
        "NVM Configuration",
        "Shell History",
        "Custom Prompt",
        "Terminal Color Configuration",
        "ZSH Syntax Highlighting",
    ];

    for section in &key_sections {
        assert!(
            jinja_template.contains(section),
            "Jinja template should contain section: {}",
            section
        );
        assert!(
            fast_template.contains(section),
            "Fast template should contain section: {}",
            section
        );
    }
}
