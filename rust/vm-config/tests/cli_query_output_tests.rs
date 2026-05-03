use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn write_config() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let config_path = temp_dir.path().join("vm.yaml");
    fs::write(
        &config_path,
        r#"
version: "2.0"
project:
  name: query-test
npm_packages:
  - prettier
  - eslint
"#,
    )
    .expect("write config");
    (temp_dir, config_path)
}

#[test]
fn query_raw_prints_string_values_to_stdout() {
    let (_temp_dir, config_path) = write_config();

    Command::cargo_bin("vm-config")
        .expect("vm-config binary")
        .args([
            "query",
            config_path.to_str().unwrap(),
            "project.name",
            "--raw",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("query-test\n"));
}

#[test]
fn query_count_prints_numbers_to_stdout() {
    let (_temp_dir, config_path) = write_config();

    Command::cargo_bin("vm-config")
        .expect("vm-config binary")
        .args(["count", config_path.to_str().unwrap(), "npm_packages"])
        .assert()
        .success()
        .stdout(predicate::str::contains("2\n"));
}
