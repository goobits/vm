use std::collections::VecDeque;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
#[cfg(unix)]
use std::process::Output;
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use tempfile::TempDir;
use vm_config::config::{BoxSpec, VmConfig, VmSettings};
use vm_provider::{
    CommandProvider, InstanceInfo, InstanceProvider, InstanceState, Provider, ProviderContext,
    ProvisioningProvider, VmStatusReport,
};

use super::{
    codex_expected, parse_codex_state, reconcile_codex, reconcile_codex_in_background, CodexState,
    CODEX_PROBE, CODEX_RECONCILE_LAUNCHER, CODEX_RECONCILE_WORKER, CODEX_REPAIR,
};

#[derive(Clone)]
struct FakeProvider {
    states: Arc<Mutex<VecDeque<&'static str>>>,
    calls: Arc<Mutex<Vec<&'static str>>>,
    commands: Arc<Mutex<Vec<Vec<String>>>>,
}

impl FakeProvider {
    fn new(states: impl IntoIterator<Item = &'static str>) -> Self {
        Self {
            states: Arc::new(Mutex::new(states.into_iter().collect())),
            calls: Arc::new(Mutex::new(Vec::new())),
            commands: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }

    fn commands(&self) -> Vec<Vec<String>> {
        self.commands.lock().unwrap().clone()
    }
}

impl CommandProvider for FakeProvider {
    fn ssh(&self, _container: Option<&str>, _relative_path: &Path) -> vm_core::error::Result<()> {
        Ok(())
    }

    fn exec(&self, _container: Option<&str>, command: &[String]) -> vm_core::error::Result<()> {
        self.calls.lock().unwrap().push("exec");
        self.commands.lock().unwrap().push(command.to_vec());
        Ok(())
    }

    fn exec_output(
        &self,
        _container: Option<&str>,
        _command: &[String],
    ) -> vm_core::error::Result<String> {
        self.calls.lock().unwrap().push("probe");
        let state = self.states.lock().unwrap().pop_front().unwrap();
        Ok(format!("VM_CODEX_STATE={state}\n"))
    }

    fn logs(&self, _container: Option<&str>) -> vm_core::error::Result<()> {
        Ok(())
    }

    fn copy(
        &self,
        _source: &str,
        _destination: &str,
        _container: Option<&str>,
    ) -> vm_core::error::Result<()> {
        Ok(())
    }
}

impl InstanceProvider for FakeProvider {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn create(&self, _context: &ProviderContext) -> vm_core::error::Result<()> {
        Ok(())
    }

    fn create_instance(
        &self,
        _instance_name: &str,
        context: &ProviderContext,
    ) -> vm_core::error::Result<()> {
        InstanceProvider::create(self, context)
    }

    fn start(
        &self,
        _container: Option<&str>,
        _context: &ProviderContext,
    ) -> vm_core::error::Result<()> {
        Ok(())
    }

    fn stop(&self, _container: Option<&str>) -> vm_core::error::Result<()> {
        Ok(())
    }

    fn destroy(
        &self,
        _container: Option<&str>,
        _context: &ProviderContext,
    ) -> vm_core::error::Result<()> {
        Ok(())
    }

    fn list_instances(&self) -> vm_core::error::Result<Vec<InstanceInfo>> {
        Ok(Vec::new())
    }

    fn status(&self, _container: Option<&str>) -> vm_core::error::Result<VmStatusReport> {
        Ok(VmStatusReport::default())
    }

    fn instance_state(&self, _container: Option<&str>) -> vm_core::error::Result<InstanceState> {
        Ok(InstanceState::Running)
    }
}

impl ProvisioningProvider for FakeProvider {
    fn provision(&self, _container: Option<&str>) -> vm_core::error::Result<()> {
        Ok(())
    }

    fn reconcile_runtime(
        &self,
        _container: Option<&str>,
        _context: &ProviderContext,
    ) -> vm_core::error::Result<()> {
        self.calls.lock().unwrap().push("runtime");
        Ok(())
    }

    fn get_sync_directory(&self) -> String {
        "/workspace".into()
    }
}

impl Provider for FakeProvider {
    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

#[cfg(unix)]
fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn fake_codex_package(directory: &TempDir, version: &str) -> std::path::PathBuf {
    let package = directory.path().join(format!("package-{version}"));
    let bin = package.join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(package.join("codex-package.json"), "{}\n").unwrap();
    fs::write(package.join("version.txt"), version).unwrap();
    write_executable(
        &bin.join("codex"),
        &format!(
            "#!/bin/sh\n\
             if test \"${{VM_CODEX_FAIL_LAUNCHER:-}}\" = \"$0\"; then exit 42; fi\n\
             printf '%s\\n' 'codex-test {version}'\n"
        ),
    );
    write_executable(
        &bin.join("codex-code-mode-host"),
        "#!/bin/sh\nprintf '%s\\n' code-mode-test\n",
    );
    package
}

#[cfg(unix)]
fn fake_codex_installer(directory: &TempDir) -> std::path::PathBuf {
    let installer = directory.path().join("install-codex.sh");
    write_executable(
        &installer,
        r#"#!/bin/sh
set -eu
target="$HOME/.local/share/codex-package"
mkdir -p "$target" "$HOME/.local/bin"
cp -R "$VM_FAKE_CODEX_PACKAGE/." "$target/"
ln -s "$target/bin/codex" "$HOME/.local/bin/codex"
"#,
    );
    installer
}

#[cfg(unix)]
fn run_codex_repair(
    directory: &TempDir,
    prefix: &Path,
    installer: &Path,
    package: &Path,
    fail_launcher: Option<&Path>,
) -> Output {
    let temporary = directory.path().join("tmp");
    fs::create_dir_all(&temporary).unwrap();
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            CODEX_REPAIR,
            "vm-codex-repair-test",
            prefix.to_str().unwrap(),
            installer.to_str().unwrap(),
        ])
        .env("HOME", directory.path().join("home"))
        .env("TMPDIR", temporary)
        .env("VM_FAKE_CODEX_PACKAGE", package);
    if let Some(launcher) = fail_launcher {
        command.env("VM_CODEX_FAIL_LAUNCHER", launcher);
    }
    command.output().unwrap()
}

#[test]
fn parses_only_explicit_codex_probe_states() {
    assert_eq!(
        parse_codex_state("shell noise\nVM_CODEX_STATE=consumable\n").unwrap(),
        CodexState::Consumable
    );
    assert_eq!(
        parse_codex_state("VM_CODEX_STATE=incomplete").unwrap(),
        CodexState::Incomplete
    );
    assert!(parse_codex_state("maybe").is_err());
}

#[test]
fn detects_vibe_runtimes_and_validates_reconciliation_scripts() {
    let mut config = VmConfig {
        preset: Some("base,vibe".into()),
        ..Default::default()
    };
    assert!(codex_expected(&config));

    config.preset = None;
    config.vm = Some(VmSettings {
        r#box: Some(BoxSpec::String("vibe-tart-linux-base".into())),
        ..Default::default()
    });
    assert!(codex_expected(&config));

    assert!(CODEX_REPAIR.contains("vm-codex-reconcile.XXXXXX"));
    assert!(CODEX_REPAIR.contains(".codex-stage.XXXXXX"));
    assert!(!CODEX_REPAIR.contains("$HOME/.codex"));
    #[cfg(unix)]
    for script in [
        CODEX_PROBE,
        CODEX_REPAIR,
        CODEX_RECONCILE_WORKER,
        CODEX_RECONCILE_LAUNCHER,
    ] {
        assert!(Command::new("/bin/sh")
            .args(["-n", "-c", script])
            .status()
            .unwrap()
            .success());
    }
}

#[test]
fn foreground_and_background_modes_use_one_guest_launch() {
    let provider = FakeProvider::new([]);
    let config = VmConfig {
        preset: Some("vibe".into()),
        ..Default::default()
    };

    reconcile_codex(&provider, "demo", &config).unwrap();
    assert!(reconcile_codex_in_background(&provider, "demo", &config).unwrap());

    assert_eq!(provider.calls(), ["exec", "exec"]);
    let commands = provider.commands();
    assert_eq!(commands[0][7], "wait");
    assert_eq!(commands[1][7], "background");
    assert_eq!(commands[0][8], "yes");
    assert_eq!(commands[0][9], "demo");
    assert!(commands[0][2].contains("nohup"));
}

#[test]
fn background_reconciliation_skips_non_vibe_environments() {
    let provider = FakeProvider::new([]);

    assert!(!reconcile_codex_in_background(&provider, "demo", &VmConfig::default()).unwrap());
    assert!(provider.calls().is_empty());
}

#[cfg(unix)]
#[test]
fn background_launcher_reuses_active_and_recent_reconciliation() {
    let directory = TempDir::new().unwrap();
    let state_home = directory.path().join("state");
    let root = state_home.join("vm-runtime");
    let launched = directory.path().join("launched");
    fs::create_dir_all(root.join("codex.lock")).unwrap();
    fs::write(
        root.join("codex.lock/pid"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();

    let run = |mode: &str| {
        Command::new("/bin/sh")
            .args([
                "-c",
                CODEX_RECONCILE_LAUNCHER,
                "vm-codex-launcher-test",
                "#!/bin/sh\nexit 0",
                "#!/bin/sh\nexit 0",
                "#!/bin/sh\n: > \"$VM_CODEX_TEST_LAUNCHED\"",
                mode,
                "yes",
                "demo",
            ])
            .env("XDG_STATE_HOME", &state_home)
            .env("VM_CODEX_TEST_LAUNCHED", &launched)
            .output()
            .unwrap()
    };

    assert!(run("background").status.success());
    assert!(!launched.exists());

    fs::remove_dir_all(root.join("codex.lock")).unwrap();
    let completed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fs::write(root.join("codex.last-success"), format!("{completed}\n")).unwrap();
    assert!(run("background").status.success());
    assert!(!launched.exists());

    assert!(run("wait").status.success());
    assert!(launched.exists());
}

#[cfg(unix)]
#[test]
fn worker_lock_makes_concurrent_and_repeated_repairs_idempotent() {
    let directory = TempDir::new().unwrap();
    let root = directory.path().join("runtime");
    fs::create_dir_all(&root).unwrap();
    let state = directory.path().join("state");
    let repairs = directory.path().join("repairs");
    fs::write(&state, "incomplete\n").unwrap();
    fs::write(&repairs, "").unwrap();
    write_executable(
        &root.join("codex-probe.sh"),
        "#!/bin/sh\nprintf 'VM_CODEX_STATE=%s\\n' \"$(cat \"$VM_CODEX_TEST_STATE\")\"\n",
    );
    write_executable(
        &root.join("codex-repair.sh"),
        "#!/bin/sh\nprintf x >> \"$VM_CODEX_TEST_REPAIRS\"\nsleep 1\nprintf '%s\\n' consumable > \"$VM_CODEX_TEST_STATE\"\n",
    );

    let worker = || {
        let mut command = Command::new("/bin/sh");
        command
            .args([
                "-c",
                CODEX_RECONCILE_WORKER,
                "vm-codex-worker-test",
                root.to_str().unwrap(),
                "yes",
                "demo",
                "wait",
            ])
            .env("VM_CODEX_TEST_STATE", &state)
            .env("VM_CODEX_TEST_REPAIRS", &repairs);
        command
    };

    let mut first = worker().spawn().unwrap();
    let mut second = worker().spawn().unwrap();
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());
    assert_eq!(fs::read_to_string(&repairs).unwrap(), "x");
    assert!(worker().status().unwrap().success());
    assert_eq!(fs::read_to_string(&repairs).unwrap(), "x");
    assert!(!root.join("codex.lock").exists());
}

#[cfg(unix)]
#[test]
fn codex_repair_is_consumable_and_rolls_back_the_complete_runtime() {
    let directory = TempDir::new().unwrap();
    let prefix = directory.path().join("prefix");
    fs::create_dir_all(&prefix).unwrap();
    let installer = fake_codex_installer(&directory);
    let first_package = fake_codex_package(&directory, "1.0.0");

    let first = run_codex_repair(&directory, &prefix, &installer, &first_package, None);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );

    let managed_root = prefix.join("lib/vm-ai-tools");
    let installed_package = managed_root.join("codex-package");
    let user_bin = directory.path().join("home/.local/bin");
    assert_eq!(
        fs::read_link(prefix.join("bin/codex")).unwrap(),
        managed_root.join("codex")
    );
    assert_eq!(
        fs::read_link(user_bin.join("codex")).unwrap(),
        managed_root.join("codex")
    );
    assert_eq!(
        fs::metadata(&installed_package)
            .unwrap()
            .permissions()
            .mode()
            & 0o005,
        0o005
    );
    assert_eq!(
        fs::metadata(&installed_package)
            .unwrap()
            .permissions()
            .mode()
            & 0o022,
        0
    );
    let repeated = run_codex_repair(&directory, &prefix, &installer, &first_package, None);
    assert!(
        repeated.status.success(),
        "{}",
        String::from_utf8_lossy(&repeated.stderr)
    );

    let second_package = fake_codex_package(&directory, "2.0.0");
    let launcher = prefix.join("bin/codex");
    let second = run_codex_repair(
        &directory,
        &prefix,
        &installer,
        &second_package,
        Some(&launcher),
    );
    assert!(!second.status.success());
    assert_eq!(
        fs::read_to_string(installed_package.join("version.txt")).unwrap(),
        "1.0.0"
    );
    let version = Command::new(&launcher).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        "codex-test 1.0.0\n"
    );
    assert!(fs::read_dir(&managed_root).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".codex-")
    }));
}

#[cfg(unix)]
#[test]
fn codex_repair_refuses_an_unmanaged_launcher() {
    let directory = TempDir::new().unwrap();
    let prefix = directory.path().join("prefix");
    let bin = prefix.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let launcher = bin.join("codex");
    write_executable(&launcher, "#!/bin/sh\nprintf '%s\\n' unmanaged\n");
    let installer = fake_codex_installer(&directory);
    let package = fake_codex_package(&directory, "1.0.0");

    let output = run_codex_repair(&directory, &prefix, &installer, &package, None);

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("Refusing to replace unmanaged launcher"));
    assert!(String::from_utf8(fs::read(&launcher).unwrap())
        .unwrap()
        .contains("unmanaged"));
}

#[cfg(unix)]
#[test]
fn codex_repair_refuses_an_unmanaged_user_launcher() {
    let directory = TempDir::new().unwrap();
    let prefix = directory.path().join("prefix");
    let user_bin = directory.path().join("home/.local/bin");
    let unmanaged = directory.path().join("home/custom/codex");
    fs::create_dir_all(&user_bin).unwrap();
    fs::create_dir_all(unmanaged.parent().unwrap()).unwrap();
    write_executable(&unmanaged, "#!/bin/sh\nprintf '%s\\n' unmanaged\n");
    std::os::unix::fs::symlink(&unmanaged, user_bin.join("codex")).unwrap();
    let installer = fake_codex_installer(&directory);
    let package = fake_codex_package(&directory, "1.0.0");

    let output = run_codex_repair(&directory, &prefix, &installer, &package, None);

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("Refusing to replace unmanaged launcher"));
    assert_eq!(fs::read_link(user_bin.join("codex")).unwrap(), unmanaged);
}

#[cfg(unix)]
#[test]
fn failed_initial_codex_repair_leaves_no_partial_runtime() {
    let directory = TempDir::new().unwrap();
    let prefix = directory.path().join("prefix");
    fs::create_dir_all(&prefix).unwrap();
    let installer = fake_codex_installer(&directory);
    let package = fake_codex_package(&directory, "1.0.0");
    let launcher = prefix.join("bin/codex");

    let output = run_codex_repair(&directory, &prefix, &installer, &package, Some(&launcher));

    assert!(!output.status.success());
    for path in [
        prefix.join("lib/vm-ai-tools/codex-package"),
        prefix.join("lib/vm-ai-tools/codex"),
        prefix.join("lib/vm-ai-tools/codex-code-mode-host"),
        launcher,
        prefix.join("bin/codex-code-mode-host"),
        directory.path().join("home/.local/bin/codex"),
        directory
            .path()
            .join("home/.local/bin/codex-code-mode-host"),
    ] {
        assert!(fs::symlink_metadata(path).is_err());
    }
}
