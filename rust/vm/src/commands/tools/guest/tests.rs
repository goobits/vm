use super::*;
use chrono::Utc;
use std::{fs, process::Command};
#[cfg(unix)]
use std::{
    io::Write,
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};
use vm_packages::ToolKind;

#[test]
fn maps_linux_and_macos_guest_targets() {
    assert_eq!(
        platform_target_from_uname("Linux\naarch64\n").unwrap(),
        "linux-arm64"
    );
    assert_eq!(
        platform_target_from_uname("Darwin\narm64\n").unwrap(),
        "darwin-arm64"
    );
    assert!(platform_target_from_uname("Plan9\nx86_64\n").is_err());
}

#[test]
fn parses_only_valid_guest_state_lines() {
    let output = format!("noise\ncodex\t1.2.3\tlinux-arm64\t{}\n", "a".repeat(64));
    let installed = parse_installed(&output);
    assert_eq!(installed["codex"].version, "1.2.3");
    assert_eq!(installed.len(), 1);
}

#[test]
fn parses_consumability_without_trusting_unknown_rows() {
    let states = parse_consumable("agent-skills\tyes\nbroken\tno\nnoise\nother\tmaybe\n");

    assert_eq!(states.get("agent-skills"), Some(&true));
    assert_eq!(states.get("broken"), Some(&false));
    assert_eq!(states.len(), 2);
    #[cfg(unix)]
    assert!(std::process::Command::new("/bin/sh")
        .args(["-n", "-c", CONSUMABLE_SCRIPT])
        .status()
        .unwrap()
        .success());
}

#[test]
fn parses_one_combined_shell_state_probe() {
    let digest = "a".repeat(64);
    let output = format!(
        "{PLATFORM_SECTION}\nLinux\naarch64\n{INSTALLED_SECTION}\nagent-skills\t1.0.0\tlinux-arm64\t{digest}\n{CONSUMABLE_SECTION}\nagent-skills\tyes\n"
    );

    let state = parse_shell_state(&output).unwrap();

    assert_eq!(state.target, "linux-arm64");
    assert_eq!(state.installed["agent-skills"].version, "1.0.0");
    assert_eq!(state.consumable.get("agent-skills"), Some(&true));
    #[cfg(unix)]
    assert!(std::process::Command::new("/bin/sh")
        .args(["-n", "-c", &shell_state_script()])
        .status()
        .unwrap()
        .success());
}

#[cfg(unix)]
#[test]
fn consumability_requires_links_to_the_recorded_release() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let data = directory.path().join("data");
    let root = data.join("vm-tools");
    let states = root.join("state");
    let releases = root.join("releases/agent-skills");
    let old_release = releases.join(format!("1.0.0-{}", "a".repeat(64)));
    let current_release = releases.join(format!("2.0.0-{}", "b".repeat(64)));
    fs::create_dir_all(old_release.join("skills")).unwrap();
    fs::create_dir_all(current_release.join("skills")).unwrap();
    fs::create_dir_all(&states).unwrap();
    fs::create_dir_all(&home).unwrap();
    let destination = home.join("skills");
    std::os::unix::fs::symlink(old_release.join("skills"), &destination).unwrap();
    fs::write(
        states.join("agent-skills.state"),
        format!("agent-skills\t2.0.0\tany\t{}\n", "b".repeat(64)),
    )
    .unwrap();
    fs::write(
        states.join("agent-skills.links"),
        format!("{}\n", destination.display()),
    )
    .unwrap();

    let inspect = || {
        Command::new("sh")
            .args(["-c", CONSUMABLE_SCRIPT])
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &data)
            .output()
            .unwrap()
    };
    assert_eq!(
        String::from_utf8_lossy(&inspect().stdout),
        "agent-skills\tno\n"
    );

    fs::remove_file(&destination).unwrap();
    std::os::unix::fs::symlink(current_release.join("skills"), &destination).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&inspect().stdout),
        "agent-skills\tyes\n"
    );
}

#[test]
fn finds_only_standalone_project_collection_checkouts() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = directory.path().join("workspace");
    let checkout = workspace.join(".claude/skills");
    let ordinary = workspace.join(".codex/skills");
    fs::create_dir_all(&checkout).unwrap();
    fs::create_dir_all(&ordinary).unwrap();
    assert!(Command::new("git")
        .args(["init", "--quiet"])
        .arg(&checkout)
        .status()
        .unwrap()
        .success());

    let output = Command::new("sh")
        .args(["-c", PROJECT_COLLECTION_OVERRIDES_SCRIPT, "test"])
        .arg(&workspace)
        .args([
            "agent-skills",
            ".claude/skills",
            "agent-skills",
            ".codex/skills",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let candidates = BTreeSet::from([
        ("agent-skills".into(), ".claude/skills".into()),
        ("agent-skills".into(), ".codex/skills".into()),
    ]);
    let overrides =
        parse_project_collection_overrides(&String::from_utf8(output.stdout).unwrap(), &candidates);

    assert_eq!(
        overrides["agent-skills"],
        BTreeSet::from([".claude/skills".into()])
    );
    assert!(Command::new("sh")
        .args(["-n", "-c", PROJECT_COLLECTION_OVERRIDES_SCRIPT])
        .status()
        .unwrap()
        .success());
}

#[test]
fn ignores_unrequested_project_override_rows() {
    let candidates = BTreeSet::from([("agent-skills".into(), ".claude/skills".into())]);
    let overrides = parse_project_collection_overrides(
        "agent-skills\t.claude/skills\nother\t.codex/skills\nnoise\n",
        &candidates,
    );

    assert_eq!(overrides.len(), 1);
    assert_eq!(overrides["agent-skills"].len(), 1);
}

#[cfg(unix)]
#[test]
fn shell_tool_reconciliation_reuses_active_and_recent_work() {
    for script in [LAUNCHER, INSTALLER] {
        assert!(Command::new("/bin/sh")
            .args(["-n", "-c", script])
            .status()
            .unwrap()
            .success());
    }

    let directory = tempfile::tempdir().unwrap();
    let data = directory.path().join("data");
    let root = data.join("vm-tools");
    let launched = directory.path().join("launched");
    fs::create_dir_all(root.join("update.lock")).unwrap();
    fs::write(
        root.join("update.lock/pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    let inner = Command::new("/bin/sh")
        .args([
            "-c",
            INSTALLER,
            "vm-tool-installer-test",
            "background-if-idle",
            "invalid manifest",
        ])
        .env("XDG_DATA_HOME", &data)
        .status()
        .unwrap();
    assert!(inner.success());

    let run_launcher = |mode: &str| {
        let mut child = Command::new("/bin/sh")
            .args([
                "-c",
                LAUNCHER,
                "vm-tool-launcher-test",
                "#!/bin/sh\n: > \"$VM_TOOL_TEST_LAUNCHED\"",
                mode,
            ])
            .env("XDG_DATA_HOME", &data)
            .env("VM_TOOL_TEST_LAUNCHED", &launched)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"test-token\n")
            .unwrap();
        child.wait_with_output().unwrap()
    };

    assert!(run_launcher("background-if-idle").status.success());
    assert!(!launched.exists());

    fs::remove_dir_all(root.join("update.lock")).unwrap();
    let completed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fs::write(root.join("update.last-success"), format!("{completed}\n")).unwrap();
    assert!(run_launcher("background-if-idle").status.success());
    assert!(!launched.exists());

    assert!(run_launcher("wait").status.success());
    assert!(launched.exists());
}

#[test]
fn manifest_keeps_a_collection_as_one_artifact() {
    let artifact = ToolArtifactRecord {
        tool: "agent-skills".into(),
        kind: ToolKind::Collection,
        version: "1.0.0".into(),
        target: "any".into(),
        artifact_digest: "a".repeat(64),
        size_bytes: 1,
        links: BTreeMap::from([
            (".claude/skills".into(), "skills".into()),
            (".codex/skills".into(), "skills".into()),
        ]),
        source_repository: "https://example.com/skills.git".into(),
        source_commit: "b".repeat(40),
        tag: "v1.0.0".into(),
        artifact_path: vm_packages::tool_artifact_path(
            "agent-skills",
            "1.0.0",
            "any",
            &"a".repeat(64),
        ),
        actor: "release".into(),
        published_at: Utc::now(),
        receipt_id: "receipt-1".into(),
    };
    let manifest = manifest(&artifact, "http://packages.internal:3080").unwrap();
    assert_eq!(manifest.lines().count(), 3);
    assert!(manifest.starts_with("agent-skills\t1.0.0\tany\tcollection\t"));
}

#[test]
fn collection_installer_retargets_managed_links_without_duplicate_copies() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let data = directory.path().join("data");
    let digest = "a".repeat(64);
    let release = data
        .join("vm-tools/releases/agent-skills")
        .join(format!("1.0.0-{digest}"))
        .join("skills");
    for skill in ["x-one", "x-two"] {
        let skill = release.join(skill);
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Test\n").unwrap();
    }
    assert!(Command::new("chmod")
        .args(["-R", "a-w"])
        .arg(release.parent().unwrap())
        .status()
        .unwrap()
        .success());
    fs::create_dir_all(home.join(".codex/skills/.system")).unwrap();
    let installer = directory.path().join("installer.sh");
    fs::write(&installer, INSTALLER).unwrap();
    let manifest = format!(
        "agent-skills\t1.0.0\tany\tcollection\t{digest}\thttp://unused\n.agents/skills\tskills\n.codex/skills\tskills"
    );

    let output = Command::new("sh")
        .arg(&installer)
        .arg("wait")
        .arg(&manifest)
        .env("HOME", &home)
        .env("XDG_DATA_HOME", &data)
        .env("CARGO_REGISTRIES_VM_TOKEN", "test-token")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home.join(".codex/skills/.system").is_dir());
    assert!(home.join(".codex/skills/x-one").is_symlink());
    assert!(home.join(".codex/skills/x-two").is_symlink());
    let links = fs::read_to_string(data.join("vm-tools/state/agent-skills.links")).unwrap();
    assert_eq!(links.lines().count(), 3);

    #[cfg(unix)]
    {
        fs::remove_file(home.join(".codex/skills/x-one")).unwrap();
        std::os::unix::fs::symlink(
            data.join("vm-tools/releases/agent-skills/missing/skills/x-one"),
            home.join(".codex/skills/x-one"),
        )
        .unwrap();
        let retry = Command::new("sh")
            .arg(&installer)
            .arg("wait")
            .arg(&manifest)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &data)
            .env("CARGO_REGISTRIES_VM_TOKEN", "test-token")
            .output()
            .unwrap();
        assert!(
            retry.status.success(),
            "{}",
            String::from_utf8_lossy(&retry.stderr)
        );
        assert!(home.join(".codex/skills/x-one/SKILL.md").is_file());

        let next_digest = "b".repeat(64);
        let next_release = data
            .join("vm-tools/releases/agent-skills")
            .join(format!("2.0.0-{next_digest}"))
            .join("skills");
        for skill in ["x-one", "x-two"] {
            let skill = next_release.join(skill);
            fs::create_dir_all(&skill).unwrap();
            fs::write(skill.join("SKILL.md"), "# Updated\n").unwrap();
        }
        assert!(Command::new("chmod")
            .args(["-R", "a-w"])
            .arg(next_release.parent().unwrap())
            .status()
            .unwrap()
            .success());
        let next_manifest = format!(
            "agent-skills\t2.0.0\tany\tcollection\t{next_digest}\thttp://unused\n.agents/skills\tskills\n.codex/skills\tskills"
        );
        let update = Command::new("sh")
            .arg(&installer)
            .arg("wait")
            .arg(&next_manifest)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &data)
            .env("CARGO_REGISTRIES_VM_TOKEN", "test-token")
            .output()
            .unwrap();
        assert!(
            update.status.success(),
            "{}",
            String::from_utf8_lossy(&update.stderr)
        );
        assert_eq!(
            fs::read_link(home.join(".agents/skills")).unwrap(),
            next_release
        );
        assert_eq!(
            fs::read_link(home.join(".codex/skills/x-one")).unwrap(),
            next_release.join("x-one")
        );
        assert!(
            fs::read_to_string(home.join(".codex/skills/x-one/SKILL.md"))
                .unwrap()
                .contains("Updated")
        );
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&next_release).unwrap().permissions().mode() & 0o222,
            0
        );
        assert_eq!(
            fs::metadata(next_release.join("x-one/SKILL.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o222,
            0
        );
        assert!(!fs::read_dir(&release)
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with(".vm-tool-")));
    }
}

#[cfg(unix)]
#[test]
fn collection_installer_replaces_writable_release_from_immutable_artifact() {
    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let data = directory.path().join("data");
    let contents = directory.path().join("contents/skills/x-one");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&contents).unwrap();
    fs::write(contents.join("SKILL.md"), "# Canonical\n").unwrap();
    let archive = directory.path().join("agent-skills.tar.gz");
    assert!(Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .args(["-C"])
        .arg(directory.path().join("contents"))
        .arg(".")
        .status()
        .unwrap()
        .success());
    let digest = vm_packages::sha256_hex(fs::read(&archive).unwrap());
    let manifest = format!(
        "agent-skills\t1.0.0\tany\tcollection\t{digest}\tfile://{}\n.codex/skills\tskills",
        archive.display()
    );
    let run = || {
        Command::new("sh")
            .args(["-c", INSTALLER, "vm-tool-installer-test", "wait"])
            .arg(&manifest)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &data)
            .env("CARGO_REGISTRIES_VM_TOKEN", "test-token")
            .output()
            .unwrap()
    };

    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let release = data
        .join("vm-tools/releases/agent-skills")
        .join(format!("1.0.0-{digest}"));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(&release).unwrap().permissions().mode() & 0o222,
        0
    );

    assert!(Command::new("chmod")
        .args(["-R", "u+w"])
        .arg(&release)
        .status()
        .unwrap()
        .success());
    fs::write(release.join("skills/x-one/SKILL.md"), "# Mutated\n").unwrap();
    let repaired = run();
    assert!(
        repaired.status.success(),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert_eq!(
        fs::read_to_string(release.join("skills/x-one/SKILL.md")).unwrap(),
        "# Canonical\n"
    );
    assert_eq!(
        fs::metadata(&release).unwrap().permissions().mode() & 0o222,
        0
    );
}

#[cfg(unix)]
#[test]
fn binary_installer_links_an_executable_from_one_immutable_release() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let home = directory.path().join("home");
    let data = directory.path().join("data");
    let binary = directory.path().join("contents/bin/release-tool");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::write(&binary, "#!/bin/sh\nprintf '%s\\n' '1.2.3'\n").unwrap();
    let mut permissions = fs::metadata(&binary).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&binary, permissions).unwrap();
    fs::copy(&binary, home.join(".local/bin/release-tool")).unwrap();
    fs::write(
        home.join(".local/bin/release-helper"),
        "#!/bin/sh\nprintf '%s\\n' 'legacy'\n",
    )
    .unwrap();
    for path in [
        home.join(".local/bin/release-tool"),
        home.join(".local/bin/release-helper"),
    ] {
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).unwrap();
    }

    let archive = directory.path().join("release-tool.tar.gz");
    assert!(Command::new("tar")
        .args(["-czf"])
        .arg(&archive)
        .args(["-C"])
        .arg(directory.path().join("contents"))
        .arg(".")
        .status()
        .unwrap()
        .success());
    let digest = vm_packages::sha256_hex(fs::read(&archive).unwrap());
    let manifest = format!(
        "release-tool\t1.2.3\tlinux-amd64\tbinary\t{digest}\tfile://{}\n.local/bin/release-helper\tbin/release-tool\n.local/bin/release-tool\tbin/release-tool",
        archive.display()
    );
    let run = || {
        Command::new("sh")
            .args(["-c", INSTALLER, "vm-tool-installer-test", "wait"])
            .arg(&manifest)
            .env("HOME", &home)
            .env("XDG_DATA_HOME", &data)
            .env("CARGO_REGISTRIES_VM_TOKEN", "test-token")
            .output()
            .unwrap()
    };

    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let installed = home.join(".local/bin/release-tool");
    let helper = home.join(".local/bin/release-helper");
    assert!(installed.is_symlink());
    assert!(helper.is_symlink());
    let output = Command::new(&installed).output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1.2.3\n");

    let release = data
        .join("vm-tools/releases/release-tool")
        .join(format!("1.2.3-{digest}"));
    assert!(fs::canonicalize(&installed)
        .unwrap()
        .starts_with(fs::canonicalize(&release).unwrap()));
    assert_ne!(
        fs::metadata(release.join("bin/release-tool"))
            .unwrap()
            .permissions()
            .mode()
            & 0o111,
        0
    );
    assert_eq!(
        fs::metadata(&release).unwrap().permissions().mode() & 0o222,
        0
    );

    let migrations = data.join("vm-tools/migrations");
    let receipts = fs::read_dir(&migrations)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(receipts.len(), 2);
    let receipt_contents = receipts
        .iter()
        .map(|receipt| fs::read_to_string(receipt).unwrap())
        .collect::<Vec<_>>();
    assert!(receipt_contents
        .iter()
        .any(|receipt| receipt.starts_with("complete\tmatched\t")));
    assert!(receipt_contents
        .iter()
        .any(|receipt| receipt.starts_with("complete\tbacked_up\t")));

    let backup_root = data.join("vm-tools/backups/release-tool");
    let backup_dirs = fs::read_dir(&backup_root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(backup_dirs.len(), 1);
    assert!(fs::read_to_string(backup_dirs[0].join("release-helper"))
        .unwrap()
        .contains("legacy"));

    fs::remove_file(&installed).unwrap();
    fs::remove_file(&helper).unwrap();
    for (path, content) in receipts.iter().zip(receipt_contents) {
        fs::write(path, content.replacen("complete\t", "pending\t", 1)).unwrap();
    }
    let resumed = run();
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    assert!(installed.is_symlink());
    assert!(helper.is_symlink());
    assert_eq!(fs::read_dir(&backup_root).unwrap().count(), 1);
    assert!(receipts.iter().all(|receipt| fs::read_to_string(receipt)
        .unwrap()
        .starts_with("complete\t")));
}
