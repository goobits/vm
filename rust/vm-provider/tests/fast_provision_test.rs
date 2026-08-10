//! Regression tests for the canonical `.zshrc` template.

use std::collections::HashMap;

use serde_json::json;
use tera::{Context, Tera};

fn render_template_placeholders(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();

    for (key, value) in vars {
        result = result.replace(key, value);
    }
    result
}

fn render_zshrc_for_test(project_path_shell: &str) -> String {
    render_zshrc_for_test_with_prompt(project_path_shell, false, false)
}

fn render_zshrc_for_test_with_prompt(
    project_path_shell: &str,
    show_git_branch: bool,
    show_timestamp: bool,
) -> String {
    let mut tera = Tera::default();
    tera.add_raw_template("zshrc", vm_provider::ZSHRC_TEMPLATE)
        .expect("zshrc template should load");

    let mut context = Context::new();
    context.insert("project_name", "test-project");
    context.insert("project_path_shell", project_path_shell);
    context.insert("project_config", &json!({}));
    context.insert("terminal_emoji", "🚀");
    context.insert("terminal_username", "vm-dev");
    context.insert("show_git_branch", &show_git_branch);
    context.insert("show_timestamp", &show_timestamp);
    context.insert(
        "terminal_colors",
        &json!({
            "foreground": "#f8f8f2",
            "red": "#ff5555",
            "green": "#50fa7b",
            "yellow": "#f1fa8c",
            "magenta": "#ff79c6",
            "cyan": "#8be9fd",
            "bright_black": "#6272a4"
        }),
    );
    context.insert("project_aliases", &Vec::<serde_json::Value>::new());
    context.insert("project_ports", &Vec::<serde_json::Value>::new());

    tera.render("zshrc", &context)
        .expect("zshrc template should render")
}

#[test]
fn test_rendered_fast_prompt_is_valid_zsh() {
    if std::process::Command::new("zsh")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let rendered = render_zshrc_for_test_with_prompt("'/workspace'", true, true);
    let temp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(temp.path(), &rendered).unwrap();
    let output = std::process::Command::new("zsh")
        .args(["-n"])
        .arg(temp.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "rendered prompt should parse: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(rendered.contains("git branch --show-current"));
    assert!(rendered.contains("${VM_GIT_PROMPT} [%*]"));
}

fn render_docker_zshrc_for_test() -> String {
    render_zshrc_for_test("'/workspace'")
}

fn render_tart_zshrc_for_test() -> String {
    render_zshrc_for_test("'/workspace'")
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
fn test_zshrc_template_does_not_source_bash_completion() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    assert!(
        !template.contains("bash_completion"),
        "zshrc should not source Bash-only NVM completion files"
    );
}

#[test]
fn test_zshrc_template_does_not_source_bashrc() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    assert!(
        !template.contains("$HOME/.bashrc"),
        "zshrc should not source Bash startup files"
    );
}

#[test]
fn test_zshrc_template_sets_prompt_after_runtime_setup() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    let runtime_pos = template
        .find("/etc/profile.d/vm-shell-runtime.sh")
        .expect("zshrc should source shared runtime setup");
    let prompt_pos = template
        .find("PROMPT='{{ terminal_emoji }}")
        .expect("zshrc should set the canonical prompt");

    assert!(
        runtime_pos < prompt_pos,
        "runtime setup should happen before prompt setup so later shell hooks cannot reset PROMPT"
    );
}

#[test]
fn test_rendered_zshrc_prompt_survives_bashrc_prompt() {
    if std::process::Command::new("zsh")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let home = tempfile::tempdir().expect("temp home should be created");
    std::fs::write(home.path().join(".zshrc"), render_docker_zshrc_for_test())
        .expect("zshrc should be written");
    std::fs::write(home.path().join(".bashrc"), "PROMPT='broken% '\n")
        .expect("bashrc should be written");

    let output = std::process::Command::new("zsh")
        .arg("-ic")
        .arg("print -r -- $PROMPT")
        .env("HOME", home.path())
        .env("USER", "developer")
        .env("LOGNAME", "developer")
        .output()
        .expect("zsh should run");

    assert!(
        output.status.success(),
        "zsh should load rendered zshrc: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let prompt = String::from_utf8_lossy(&output.stdout);
    assert!(
        prompt.contains("🚀 vm-dev %c"),
        "canonical prompt should win, got: {prompt}"
    );
    assert!(
        !prompt.contains("broken"),
        "bashrc prompt should not override zsh prompt, got: {prompt}"
    );
}

#[test]
fn test_rendered_docker_zshrc_targets_workspace() {
    let rendered = render_docker_zshrc_for_test();

    assert!(rendered.contains("VM_PROJECT_PATH='/workspace'"));
    assert!(rendered.contains("alias dev='cd \"$VM_PROJECT_PATH\" && ls'"));
    assert!(rendered.contains("PROMPT='🚀 vm-dev %c"));
}

#[test]
fn test_rendered_tart_zshrc_targets_workspace() {
    let rendered = render_tart_zshrc_for_test();

    assert!(rendered.contains("VM_PROJECT_PATH='/workspace'"));
    assert!(rendered.contains("alias dev='cd \"$VM_PROJECT_PATH\" && ls'"));
    assert!(rendered.contains("PROMPT='🚀 vm-dev %c"));
}

#[test]
fn test_zshrc_template_has_expected_jinja_variables() {
    let template = vm_provider::ZSHRC_TEMPLATE;
    let expected_variables = [
        "{{ project_name }}",
        "{{ terminal_emoji }}",
        "{{ terminal_username }}",
        "{{ project_path_shell }}",
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
    vars.insert("{{ project_path_shell }}", "'/workspace'");

    let result = render_template_placeholders(template, &vars);

    assert!(result.contains("test-project"));
    assert!(result.contains("🚀"));
    assert!(result.contains("developer"));
    assert!(result.contains("'/workspace'"));
}

#[test]
fn test_zshrc_template_uses_fast_lazy_shell_paths() {
    let template = vm_provider::ZSHRC_TEMPLATE;

    assert!(template.contains("vm_update_git_prompt()"));
    assert!(template.contains("git branch --show-current"));
    assert!(!template.contains("git branch 2>/dev/null |"));
    assert!(!template.contains("$(date "));
    assert!(template.contains("[%*]"));
    assert!(template.contains("_vm_load_nvm()"));
    assert!(
        template.find("_vm_load_nvm()").unwrap()
            < template.find("\\. \"$NVM_DIR/nvm.sh\"").unwrap()
    );
    assert!(template.contains("$HOME/.vm_runtime_env"));
    assert!(!template.contains("PYTHONDONTWRITEBYTECODE"));
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
fn test_zshrc_template_checks_ai_home_writability() {
    let rendered = render_docker_zshrc_for_test();

    assert!(rendered.contains("vm_home_is_writable()"));
    assert!(rendered.contains(".vm-home-write-test"));
    assert!(rendered.contains("ERROR: HOME is not writable"));
    assert!(rendered.contains("$HOME/.claude/projects"));
    assert!(rendered.contains("$HOME/.claude/sessions"));
    assert!(rendered.contains("$HOME/.claude.json"));
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

#[test]
fn test_docker_database_client_installs_do_not_require_python_apt() {
    let playbook = vm_provider::resources::ANSIBLE_PLAYBOOK;
    let client_tools_pos = playbook
        .find("Install database client tools for Docker")
        .expect("playbook should install Docker database client tools");
    let docker_pos = playbook[client_tools_pos..]
        .find("Install Docker")
        .expect("playbook should install Docker after client tools");
    let client_tools_block = &playbook[client_tools_pos..client_tools_pos + docker_pos];

    assert!(client_tools_block.contains("apt-get install"));
    assert!(client_tools_block.contains("postgresql-client"));
    assert!(client_tools_block.contains("redis-tools"));
    assert!(client_tools_block.contains("mongodb-clients"));
    assert!(
        !client_tools_block.contains("\n        apt:"),
        "Docker client tools must avoid Ansible apt module because snapshots may lack python3-apt"
    );
}

#[test]
fn test_docker_engine_install_does_not_require_python_apt() {
    let playbook = vm_provider::resources::ANSIBLE_PLAYBOOK;
    let docker_pos = playbook
        .find("Install Docker")
        .expect("playbook should install Docker");
    let user_group_pos = playbook[docker_pos..]
        .find("Add user to docker group")
        .expect("playbook should configure the Docker group after installation");
    let docker_block = &playbook[docker_pos..docker_pos + user_group_pos];

    assert!(docker_block.contains("apt-get install"));
    assert!(docker_block.contains("docker.io docker-compose"));
    assert!(docker_block.contains("command -v docker"));
    assert!(
        !docker_block.contains("\n        apt:"),
        "Docker installation must avoid Ansible apt module because snapshots may expose a non-system Python"
    );
}
