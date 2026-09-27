use assert_cmd::cargo::cargo_bin;
use serde_json::Value;
use std::fs;
use std::process::Command;

#[test]
fn json_plans_are_read_only_redacted_and_report_validation_errors() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vm.yaml");
    let original = "project:\n  name: example\nprovider: docker\nvm:\n  memory: '2048'\n";
    fs::write(&path, original).unwrap();

    let run = |args: &[&str]| {
        Command::new(cargo_bin!("vm"))
            .arg("--config")
            .arg(&path)
            .args(args)
            .current_dir(directory.path())
            .env("HOME", directory.path())
            .env("USERPROFILE", directory.path())
            .env("VM_TOOL_DIR", directory.path().join(".vm"))
            .env("CI", "1")
            .output()
            .unwrap()
    };

    let set = run(&["config", "set", "vm.memory", "4096", "--dry-run", "--json"]);
    assert!(
        set.status.success(),
        "{}",
        String::from_utf8_lossy(&set.stderr)
    );
    let set_json: Value = serde_json::from_slice(&set.stdout).unwrap();
    assert_eq!(set_json["command"], "config set");
    assert_eq!(set_json["data"]["planned"], true);
    assert_eq!(set_json["data"]["target"], "project");
    assert_eq!(set_json["data"]["file_changes"][0]["action"], "change");
    assert_eq!(set_json["data"]["file_changes"][0]["field"], "vm.memory");
    assert!(!String::from_utf8_lossy(&set.stdout).contains("4096"));
    assert!(!String::from_utf8_lossy(&set.stdout).contains(&directory.path().display().to_string()));
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    let unset = run(&["config", "unset", "vm.memory", "--dry-run", "--json"]);
    assert!(unset.status.success());
    let unset_json: Value = serde_json::from_slice(&unset.stdout).unwrap();
    assert_eq!(unset_json["data"]["file_changes"][0]["action"], "remove");
    assert_eq!(fs::read_to_string(&path).unwrap(), original);

    let invalid = run(&["config", "set", "vm.memroy", "4096", "--dry-run", "--json"]);
    assert!(!invalid.status.success());
    let error_json: Value = serde_json::from_slice(&invalid.stdout).unwrap();
    assert_eq!(error_json["ok"], false);
    assert_eq!(error_json["command"], "config set");
    assert_eq!(error_json["errors"][0]["target"], "vm.memroy");
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}
