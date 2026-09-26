use assert_cmd::cargo::cargo_bin;
use std::fs;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

fn run(temp_dir: &TempDir, args: &[&str]) -> Output {
    command(temp_dir, args).output().unwrap()
}

fn command(temp_dir: &TempDir, args: &[&str]) -> Command {
    let mut command = Command::new(cargo_bin!("vm"));
    command
        .args(args)
        .current_dir(temp_dir.path())
        .env("HOME", temp_dir.path())
        .env("VM_TOOL_DIR", temp_dir.path().join(".vm"))
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "host")
        .env("CI", "1");
    command
}

fn run_storage(temp_dir: &TempDir, args: &[&str]) -> Output {
    command(temp_dir, args)
        .env("PATH", temp_dir.path())
        .env("TART_HOME", temp_dir.path().join("tart"))
        .output()
        .unwrap()
}

#[test]
fn shell_requires_a_terminal_before_resolving_an_environment() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(&temp_dir, &["shell"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires a terminal"));
}

#[test]
fn fleet_exec_requires_an_explicit_framing_mode() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(&temp_dir, &["exec", "--all-envs", "--", "true"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires --output"));
}

#[test]
fn single_exec_rejects_fleet_output_mode() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(&temp_dir, &["exec", "--output", "grouped", "--", "true"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("only available for multi-environment")
    );
}

#[test]
fn secret_set_requires_a_secure_input_source_without_starting_services() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(&temp_dir, &["secrets", "set", "API_TOKEN"]);
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    assert!(stderr.contains("Use --stdin or --file"), "{stderr}");
    assert!(!temp_dir.path().join(".vm").join("secrets").exists());
}

#[test]
fn secret_remove_requires_confirmation_before_starting_services() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(&config, "project:\n  name: secure-test\nprovider: docker\n").unwrap();

    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "secrets",
            "remove",
            "API_TOKEN",
        ],
    );

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("--yes"));
    assert!(!temp_dir.path().join(".vm/services.json").exists());
    assert!(!temp_dir.path().join(".vm/secrets").exists());
}

#[test]
fn generic_dry_run_is_rejected_until_a_real_plan_exists() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(&temp_dir, &["--dry-run", "packages", "open", "auth"]);
    assert!(!output.status.success());
    assert!(!temp_dir.path().join(".vm").exists());
}

#[test]
fn managed_guest_guard_prints_the_exact_host_command() {
    let temp_dir = TempDir::new().unwrap();
    let output = Command::new(cargo_bin!("vm"))
        .args(["tools", "update", "--env", "dev"])
        .current_dir(temp_dir.path())
        .env("HOME", temp_dir.path())
        .env("VM_MANAGED_GUEST", "1")
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "guest")
        .env("CI", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("Run on the host: vm tools update --env dev"));
}

#[test]
fn application_errors_are_rendered_once_on_stderr() {
    let temp_dir = TempDir::new().unwrap();
    for (name, contents, has_cause) in [
        ("broken.yaml", "project: [", true),
        (
            "invalid.yaml",
            "version: '2.0'\nprovider: docker\nproject:\n  name: 'bad name'\n",
            false,
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
        assert_eq!(output.status.code(), Some(2), "{stderr}");
        assert!(stdout.is_empty(), "{stdout}");
        assert_eq!(stderr.matches("Error:").count(), 1, "{stderr}");
        assert_eq!(stderr.contains("Cause:"), has_cause, "{stderr}");
        assert!(!stderr.contains("\u{1b}["));
    }
}

#[test]
fn system_info_json_is_one_versioned_envelope() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(&temp_dir, &["system", "info", "--json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "system info");
    assert_eq!(value["ok"], true);
    assert!(value["data"]["version"].is_string());
    assert_eq!(value["data"]["client_version"], value["data"]["version"]);
    assert!(value["data"]["controller_version"].is_null());
    assert!(value["data"]["providers"].is_object());
    assert_eq!(value["data"]["config_schema_version"], "2.0");
    assert_eq!(value["data"]["output_schema_version"], 1);
    assert!(value["data"]["executable"].is_string());
    assert_eq!(value["data"]["managed_installation"], false);
    assert!(value["data"]["installed_version"].is_null());
    assert_eq!(value["errors"].as_array().unwrap().len(), 0);
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
}

#[test]
fn snapshot_json_reads_are_scoped_and_redacted() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(&config, "project:\n  name: demo\nprovider: docker\n").unwrap();
    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "snapshots",
            "list",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "snapshots list");
    assert_eq!(value["data"], serde_json::json!([]));
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/snapshots-list-empty.json")).unwrap();
    assert_eq!(value, fixture);

    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "snapshots",
            "show",
            "missing",
            "--json",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "snapshots show");
    assert_eq!(value["ok"], false);
    assert_eq!(value["errors"][0]["code"], "invalid_request");
    assert_eq!(value["errors"][0]["target"], "missing");

    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "snapshots",
            "remove",
            "missing",
            "--json",
            "--yes",
        ],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "snapshots remove");
    assert_eq!(value["ok"], false);
}

#[test]
fn storage_json_omits_owner_paths() {
    let temp_dir = TempDir::new().unwrap();
    let output = run_storage(&temp_dir, &["system", "storage", "list", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "system storage list");
    assert!(value["data"].is_array());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(temp_dir.path().to_str().unwrap()));
}

#[test]
fn storage_remove_json_errors_have_one_targeted_envelope() {
    let temp_dir = TempDir::new().unwrap();
    let output = run_storage(
        &temp_dir,
        &["system", "storage", "remove", "missing", "--json", "--yes"],
    );
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "system storage remove");
    assert_eq!(value["ok"], false);
    assert_eq!(value["errors"][0]["target"], "missing");
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn storage_json_reports_unreachable_provider_without_claiming_complete_inventory() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let docker = temp_dir.path().join("docker");
    fs::write(
        &docker,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then exit 0; fi\necho 'daemon unavailable' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(docker, fs::Permissions::from_mode(0o755)).unwrap();

    for (args, target) in [
        (vec!["system", "storage", "list", "--json"], "docker"),
        (
            vec![
                "system",
                "storage",
                "remove",
                "docker:volume:owned",
                "--json",
                "--yes",
            ],
            "docker:volume:owned",
        ),
    ] {
        let output = run_storage(&temp_dir, &args);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["ok"], false);
        assert!(value["data"].is_null());
        assert_eq!(value["errors"].as_array().unwrap().len(), 1);
        assert_eq!(value["errors"][0]["target"], target);
        assert!(value["errors"][0]["message"]
            .as_str()
            .unwrap()
            .contains("daemon unavailable"));
    }
}

#[cfg(unix)]
#[test]
fn storage_inventory_tolerates_only_confirmed_disappearance_during_inspection() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = TempDir::new().unwrap();
    let docker = temp_dir.path().join("docker");
    let script = r#"#!/bin/sh
case "$1 $2" in
  '--version ') exit 0 ;;
  'volume ls') printf 'gone-volume\n'; exit 0 ;;
  'image ls') printf 'gone-image\n'; exit 0 ;;
  'volume inspect') printf '%s\n' 'VOLUME_ERROR' >&2; exit 1 ;;
  'image inspect') printf '%s\n' 'IMAGE_ERROR' >&2; exit 1 ;;
  *) echo 'unexpected command' >&2; exit 1 ;;
esac
"#;
    let missing_volume = "Error response from daemon: get gone-volume: no such volume";
    let missing_image = "Error response from daemon: No such image: gone-image";
    for (volume_error, image_error, succeeds) in [
        (missing_volume, missing_image, true),
        ("Cannot connect to the Docker daemon", missing_image, false),
        (missing_volume, "permission denied", false),
        (
            "Error response from daemon: get other-volume: no such volume",
            missing_image,
            false,
        ),
        (
            missing_volume,
            "No such image: gone-image; daemon unavailable",
            false,
        ),
    ] {
        fs::write(
            &docker,
            script
                .replace("VOLUME_ERROR", volume_error)
                .replace("IMAGE_ERROR", image_error),
        )
        .unwrap();
        fs::set_permissions(&docker, fs::Permissions::from_mode(0o755)).unwrap();
        let output = run_storage(&temp_dir, &["system", "storage", "list", "--json"]);
        assert_eq!(
            output.status.success(),
            succeeds,
            "{volume_error}; {image_error}"
        );
        assert!(output.stderr.is_empty());
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["ok"], succeeds);
        if succeeds {
            assert_eq!(value["data"], serde_json::json!([]));
        } else {
            assert_eq!(output.status.code(), Some(1));
            assert!(value["data"].is_null());
            assert_eq!(value["errors"][0]["target"], "docker");
        }
    }
}

#[test]
fn tunnel_list_json_is_scoped_and_uses_one_redacted_envelope() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(&config, "project:\n  name: demo\nprovider: docker\n").unwrap();
    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "tunnels",
            "list",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "tunnels list");
    assert_eq!(value["data"]["project"], "demo");
    assert_eq!(value["data"]["tunnels"], serde_json::json!([]));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(config.to_str().unwrap()));
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );

    fs::write(&config, "project: [").unwrap();
    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "tunnels",
            "list",
            "--json",
        ],
    );
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "tunnels list");
    assert_eq!(value["ok"], false);
    assert_eq!(value["errors"].as_array().unwrap().len(), 1);
}

#[test]
fn plugin_json_reads_redact_content_and_route_errors() {
    let temp_dir = TempDir::new().unwrap();
    let plugin_dir = temp_dir.path().join(".vm/plugins/services/demo");
    fs::create_dir_all(&plugin_dir).unwrap();
    fs::write(
        plugin_dir.join("plugin.yaml"),
        "name: demo\nversion: 1.0.0\nplugin_type: service\n",
    )
    .unwrap();
    fs::write(plugin_dir.join("service.yaml"), "image: example/api:1\nports: ['8080:80']\nvolumes: ['/private/data:/data']\nenvironment: {TOKEN: supersecret}\ncommand: ['--password=secret']\n").unwrap();

    let output = run(&temp_dir, &["plugins", "list", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "plugins list");
    assert_eq!(value["data"]["plugins"][0]["name"], "demo");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(plugin_dir.to_str().unwrap()));

    let output = run(&temp_dir, &["plugins", "show", "demo", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "plugins show");
    assert_eq!(value["data"]["details"]["kind"], "service");
    assert_eq!(value["data"]["details"]["environment_variable_count"], 1);
    for secret in [
        "supersecret",
        "--password=secret",
        "/private/data",
        plugin_dir.to_str().unwrap(),
    ] {
        assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    }

    let output = run(&temp_dir, &["plugins", "show", "missing", "--json"]);
    assert!(!output.status.success());
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "plugins show");
    assert_eq!(value["ok"], false);
    assert_eq!(value["errors"][0]["target"], "missing");
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
}

#[test]
fn list_json_reports_declared_environments_without_leaking_config_secrets() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(
        &config,
        "version: '2.0'\nprovider: docker\nproject:\n  name: output-test\n  default_environment: dev\nenvironments:\n  dev:\n    provider: docker\n    image: ubuntu:24.04\nenvironment:\n  API_TOKEN: top-secret\n",
    )
    .unwrap();
    let output = run(
        &temp_dir,
        &["--config", config.to_str().unwrap(), "list", "--json"],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "list");
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["count"], 1);
    assert_eq!(value["data"]["environments"][0]["is_default"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("top-secret"));
    assert!(output.stderr.is_empty());
}

#[test]
fn status_json_errors_use_one_envelope() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(&config, "project: [").unwrap();
    let output = run(
        &temp_dir,
        &["--config", config.to_str().unwrap(), "status", "--json"],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "status");
    assert_eq!(value["ok"], false);
    assert_eq!(value["errors"].as_array().unwrap().len(), 1);
    assert_eq!(
        output.stdout.iter().filter(|byte| **byte == b'\n').count(),
        1
    );
    assert!(output.stderr.is_empty());
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
fn config_show_never_materializes_package_credentials() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(
        &config,
        "version: '2.0'\nprovider: docker\nproject:\n  name: output-test\n",
    )
    .unwrap();
    let appliance = temp_dir.path().join(".vm/infrastructure/packages");
    fs::create_dir_all(&appliance).unwrap();
    fs::write(
        appliance.join("state.json"),
        r#"{
  "engine": "docker",
  "gateway_url": "http://127.0.0.1:3080",
  "gateway_port": 3080,
  "registry_image": "registry/image:1",
  "job_image": "jobs/image:1",
  "controller_version": "1"
}

"#,
    )
    .unwrap();
    fs::write(appliance.join("read-token"), "do-not-print-package-token").unwrap();

    let output = run(
        &temp_dir,
        &["--config", config.to_str().unwrap(), "config", "show"],
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(output.status.success(), "{stderr}");
    assert!(!stdout.contains("do-not-print-package-token"));
    assert!(!stdout.contains("NPM_CONFIG_REGISTRY"));
    assert!(!stdout.contains("CARGO_REGISTRIES_VM_TOKEN"));
}

#[test]
fn config_show_json_is_redacted_and_identifies_sources() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    fs::write(
        &config,
        "version: '2.0'\nprovider: docker\nproject:\n  name: output-test\nenvironment:\n  API_TOKEN: top-secret\n",
    )
    .unwrap();
    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "config",
            "show",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["command"], "config show");
    assert_eq!(value["ok"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("top-secret"));
    assert!(value["data"]["sources"].is_object());

    let output = run(
        &temp_dir,
        &[
            "--config",
            config.to_str().unwrap(),
            "config",
            "get",
            "environment.API_TOKEN",
            "--json",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["command"], "config get");
    assert_eq!(value["data"]["value"], "<redacted>");
    assert!(value["data"]["source"].is_string());
}

#[test]
fn config_show_tolerates_a_closed_stdout_pipe() {
    let temp_dir = TempDir::new().unwrap();
    let config = temp_dir.path().join("vm.yaml");
    let mut contents = String::from(
        "version: '2.0'\nprovider: docker\nproject:\n  name: output-test\nenvironment:\n",
    );
    for index in 0..5_000 {
        contents.push_str(&format!("  OUTPUT_{index}: '{}'\n", "x".repeat(100)));
    }
    fs::write(&config, contents).unwrap();

    let mut child = Command::new(cargo_bin!("vm"))
        .args(["--config", config.to_str().unwrap(), "config", "show"])
        .current_dir(temp_dir.path())
        .env("HOME", temp_dir.path())
        .env("VM_TOOL_DIR", temp_dir.path().join(".vm"))
        .env("VM_TEST_MODE", "1")
        .env("VM_TEST_COMMAND_CONTEXT", "host")
        .env("CI", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(output.status.success(), "{stderr}");
    assert!(!stderr.contains("Broken pipe"), "{stderr}");
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn every_public_command_has_clean_help() {
    let temp_dir = TempDir::new().unwrap();
    for command in [
        "start",
        "list",
        "shell",
        "exec",
        "logs",
        "copy",
        "stop",
        "status",
        "restart",
        "remove",
        "snapshots",
        "packages",
        "tools",
        "config",
        "tunnels",
        "doctor",
        "plugins",
        "system",
        "db",
        "secrets",
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

#[test]
fn direct_tool_publication_is_not_a_public_command() {
    let temp_dir = TempDir::new().unwrap();
    let output = run(&temp_dir, &["tools", "publish", "--help"]);
    let stderr = String::from_utf8(output.stderr).unwrap();

    assert!(!output.status.success());
    assert!(
        stderr.contains("unrecognized subcommand 'publish'"),
        "{stderr}"
    );
}
