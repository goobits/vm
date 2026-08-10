use assert_cmd::cargo::cargo_bin;
use std::fs;
use std::process::{Command, Output};
use tempfile::TempDir;

fn run(temp_dir: &TempDir, args: &[&str]) -> Output {
    Command::new(cargo_bin!("vm"))
        .args(args)
        .current_dir(temp_dir.path())
        .env("HOME", temp_dir.path())
        .env("VM_TOOL_DIR", temp_dir.path().join(".vm"))
        .env("VM_TEST_MODE", "1")
        .env("CI", "1")
        .output()
        .unwrap()
}

#[test]
fn dry_run_redacts_secret_values_and_changes_nothing() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(
        &temp_dir,
        &["--dry-run", "secret", "add", "API_TOKEN", "do-not-print-me"],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(output.status.success(), "{stderr}");
    assert!(stdout.contains("values redacted"));
    assert!(stdout.contains("No changes made."));
    assert!(!stdout.contains("do-not-print-me"));
    assert!(!stderr.contains("do-not-print-me"));
    assert!(!temp_dir.path().join(".vm").join("secrets").exists());
}

#[test]
fn application_errors_are_rendered_once_on_stderr() {
    let temp_dir = TempDir::new().unwrap();
    for (name, contents) in [
        ("broken.yaml", "project: ["),
        (
            "invalid.yaml",
            "version: '2.0'\nprovider: docker\nproject:\n  name: 'bad name'\n",
        ),
    ] {
        let config = temp_dir.path().join(name);
        fs::write(&config, contents).unwrap();
        let output = run(
            &temp_dir,
            &["--config", config.to_str().unwrap(), "config", "validate"],
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();

        assert!(!output.status.success());
        assert!(stdout.is_empty(), "{stdout}");
        assert_eq!(stderr.matches("Error:").count(), 1, "{stderr}");
        assert!(!stderr.contains("\u{1b}["));
    }
}

#[test]
fn compose_render_is_raw_redacted_stdout() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(
        &config,
        r#"
version: "2.0"
provider: docker
project:
  name: output-test
environment:
  API_TOKEN: top-secret
host_sync:
  worktrees:
    enabled: false
"#,
    )
    .unwrap();

    let output = run(
        &temp_dir,
        &["--config", config.to_str().unwrap(), "config", "render"],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(output.status.success(), "{stderr}");
    let rendered: serde_yaml_ng::Value = serde_yaml_ng::from_str(&stdout).unwrap();
    assert!(rendered.get("services").is_some(), "{stdout}");
    assert!(stdout.contains("API_TOKEN=<redacted>"));
    assert!(!stdout.contains("top-secret"));
    assert!(!stdout.contains(temp_dir.path().to_str().unwrap()));
    assert!(!stdout.contains("\u{1b}["));
    assert!(stderr.is_empty(), "{stderr}");
}

#[test]
fn every_public_command_has_clean_help() {
    let temp_dir = TempDir::new().unwrap();
    for command in [
        "start", "run", "list", "shell", "exec", "logs", "copy", "stop", "status", "restart",
        "remove", "save", "revert", "package", "config", "tunnel", "doctor", "plugin", "system",
        "db", "fleet", "secret",
    ] {
        let output = run(&temp_dir, &[command, "--help"]);
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();

        assert!(output.status.success(), "{command}: {stderr}");
        assert!(stdout.contains("Usage:"), "{command}: {stdout}");
        assert!(!stdout.contains("\u{1b}["), "{command}: {stdout}");
        assert!(stderr.is_empty(), "{command}: {stderr}");
    }
}
