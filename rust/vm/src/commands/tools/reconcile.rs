use vm_config::{config::BoxSpec, config::VmConfig, GlobalConfig};
use vm_core::{vm_progress, vm_success};
use vm_provider::{Provider, ProviderContext};

use crate::error::{VmError, VmResult};

use super::super::command_context::RuntimeSubject;

const CODEX_PROBE: &str = r#"
resolve_path() {
  candidate=$1
  if command -v realpath >/dev/null 2>&1; then
    realpath "$candidate" 2>/dev/null && return 0
  fi
  if readlink -f "$candidate" >/dev/null 2>&1; then
    readlink -f "$candidate"
    return
  fi
  depth=0
  while test -L "$candidate"; do
    depth=$((depth + 1))
    test "$depth" -le 40 || return 1
    target=$(readlink "$candidate") || return 1
    case "$target" in
      /*) candidate=$target ;;
      *) candidate="$(dirname "$candidate")/$target" ;;
    esac
  done
  parent=$(CDPATH= cd -P "$(dirname "$candidate")" 2>/dev/null && pwd) || return 1
  printf '%s/%s\n' "$parent" "$(basename "$candidate")"
}
codex_path="$(command -v codex 2>/dev/null || true)"
if test -z "$codex_path"; then
  printf '%s\n' VM_CODEX_STATE=absent
  exit 0
fi
if test ! -x "$codex_path"; then
  printf '%s\n' VM_CODEX_STATE=incomplete
  exit 0
fi
resolved="$(resolve_path "$codex_path" 2>/dev/null || printf '%s' "$codex_path")"
bin_dir="$(dirname "$resolved")"
package_dir="$(dirname "$bin_dir")"
if test -f "$package_dir/codex-package.json" \
  && test -x "$bin_dir/codex-code-mode-host" \
  && "$resolved" --version >/dev/null 2>&1; then
  printf '%s\n' VM_CODEX_STATE=consumable
else
  printf '%s\n' VM_CODEX_STATE=incomplete
fi
"#;

const CODEX_REPAIR: &str = r#"
set -eu

system_prefix=${1:-/usr/local}
installer_override=${2:-}
case "$system_prefix" in
  /*) ;;
  *)
    printf '%s\n' 'Codex repair requires an absolute installation prefix' >&2
    exit 1
    ;;
esac
root="$system_prefix/lib/vm-ai-tools"
bin_root="$system_prefix/bin"
target="$root/codex-package"

resolve_path() {
  candidate=$1
  if command -v realpath >/dev/null 2>&1; then
    realpath "$candidate" 2>/dev/null && return 0
  fi
  if readlink -f "$candidate" >/dev/null 2>&1; then
    readlink -f "$candidate"
    return
  fi
  depth=0
  while test -L "$candidate"; do
    depth=$((depth + 1))
    test "$depth" -le 40 || return 1
    target=$(readlink "$candidate") || return 1
    case "$target" in
      /*) candidate=$target ;;
      *) candidate="$(dirname "$candidate")/$target" ;;
    esac
  done
  parent=$(CDPATH= cd -P "$(dirname "$candidate")" 2>/dev/null && pwd) || return 1
  printf '%s/%s\n' "$parent" "$(basename "$candidate")"
}

run_install() {
  if test -d "$system_prefix" && test -w "$system_prefix"; then
    "$@"
  elif test ! -e "$system_prefix" && test -w "$(dirname "$system_prefix")"; then
    "$@"
  elif test "$(id -u)" -eq 0; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo -n "$@"
  else
    printf '%s\n' 'Codex repair requires root or passwordless sudo' >&2
    return 1
  fi
}

path_exists() {
  run_install test -e "$1" || run_install test -L "$1"
}

require_managed_launcher() {
  launcher=$1
  shift
  if ! path_exists "$launcher"; then
    return 0
  fi
  if ! run_install test -L "$launcher"; then
    printf 'Refusing to replace unmanaged launcher: %s\n' "$launcher" >&2
    return 1
  fi
  launcher_target="$(run_install readlink "$launcher")"
  for managed_target in "$@"; do
    if test "$launcher_target" = "$managed_target"; then
      return 0
    fi
  done
  printf 'Refusing to replace unmanaged launcher: %s -> %s\n' \
    "$launcher" "$launcher_target" >&2
  return 1
}

require_managed_launcher \
  "$bin_root/codex" "$root/codex" "$target/bin/codex"
require_managed_launcher \
  "$bin_root/codex-code-mode-host" \
  "$root/codex-code-mode-host" "$target/bin/codex-code-mode-host"

temporary="$(mktemp -d "${TMPDIR:-/tmp}/vm-codex-reconcile.XXXXXX")"
stage=""
backup=""
rollback_needed=no

backup_path() {
  source_path=$1
  backup_name=$2
  if path_exists "$source_path"; then
    run_install mv "$source_path" "$backup/$backup_name"
  else
    run_install touch "$backup/.absent-$backup_name"
  fi
}

restore_path() {
  backup_name=$1
  destination=$2
  if path_exists "$backup/$backup_name"; then
    run_install rm -rf "$destination" >/dev/null 2>&1 || true
    run_install mv "$backup/$backup_name" "$destination" \
      >/dev/null 2>&1 || true
  elif path_exists "$backup/.absent-$backup_name"; then
    run_install rm -rf "$destination" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test "$rollback_needed" = yes; then
    restore_path package "$target"
    restore_path root-codex "$root/codex"
    restore_path root-code-mode "$root/codex-code-mode-host"
    restore_path bin-codex "$bin_root/codex"
    restore_path bin-code-mode "$bin_root/codex-code-mode-host"
  fi
  rm -rf "$temporary"
  if test -n "$stage"; then
    run_install rm -rf "$stage" >/dev/null 2>&1 || true
  fi
  if test -n "$backup"; then
    run_install rm -rf "$backup" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

export HOME="$temporary/home"
mkdir -p "$HOME/.local/bin"
export PATH="$HOME/.local/bin:$PATH"
installer="$temporary/install-codex.sh"
if test -n "$installer_override"; then
  cp "$installer_override" "$installer"
else
  curl --fail --silent --show-error --location \
    --connect-timeout 10 --max-time 600 --retry 2 \
    --output "$installer" https://chatgpt.com/codex/install.sh
fi
sh "$installer"
hash -r

codex_path="$(command -v codex)"
resolved="$(resolve_path "$codex_path")"
bin_dir="$(dirname "$resolved")"
package_source="$(dirname "$bin_dir")"
test -f "$package_source/codex-package.json"
test -x "$bin_dir/codex-code-mode-host"
"$resolved" --version >/dev/null

run_install install -d -m 0755 "$root" "$bin_root"
stage="$(run_install mktemp -d "$root/.codex-stage.XXXXXX")"
run_install cp -R "$package_source/." "$stage/"
run_install chmod -R go-w,a+rX "$stage"
test -f "$stage/codex-package.json"
test -x "$stage/bin/codex"
test -x "$stage/bin/codex-code-mode-host"
"$stage/bin/codex" --version >/dev/null

backup="$(run_install mktemp -d "$root/.codex-backup.XXXXXX")"
rollback_needed=yes
backup_path "$target" package
backup_path "$root/codex" root-codex
backup_path "$root/codex-code-mode-host" root-code-mode
backup_path "$bin_root/codex" bin-codex
backup_path "$bin_root/codex-code-mode-host" bin-code-mode

run_install mv "$stage" "$target"
stage=""

run_install ln -s "$target/bin/codex" "$root/codex"
run_install ln -s "$target/bin/codex-code-mode-host" "$root/codex-code-mode-host"
run_install ln -s "$root/codex" "$bin_root/codex"
run_install ln -s "$root/codex-code-mode-host" "$bin_root/codex-code-mode-host"
test -x "$bin_root/codex-code-mode-host"
"$bin_root/codex" --version >/dev/null
rollback_needed=no
run_install rm -rf "$backup"
backup=""
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CodexState {
    Absent,
    Incomplete,
    Consumable,
}

pub(super) fn environment(subject: &RuntimeSubject) -> VmResult<()> {
    reconcile_for(
        subject.provider.as_ref(),
        &subject.target,
        &subject.config,
        &subject.global_config,
    )
}

fn reconcile_for(
    provider: &dyn Provider,
    environment: &str,
    config: &VmConfig,
    global_config: &GlobalConfig,
) -> VmResult<()> {
    let context = ProviderContext::default().with_config(global_config.clone());
    provider
        .reconcile_runtime(Some(environment), &context)
        .map_err(VmError::from)?;

    let before = codex_state(provider, environment)?;
    if before == CodexState::Consumable || (before == CodexState::Absent && !codex_expected(config))
    {
        return Ok(());
    }

    vm_progress!("Repairing the Codex standalone runtime in '{environment}'...");
    provider
        .exec(
            Some(environment),
            &["sh".into(), "-c".into(), CODEX_REPAIR.into()],
        )
        .map_err(VmError::from)?;
    if codex_state(provider, environment)? != CodexState::Consumable {
        return Err(VmError::validation(
            "Codex repair completed without a consumable standalone runtime",
            Some(format!(
                "Run `vm exec {environment} -- sh -lc 'command -v codex && command -v codex-code-mode-host'`"
            )),
        ));
    }
    vm_success!("Codex standalone runtime is consumable");
    Ok(())
}

pub(super) fn codex_state(provider: &dyn Provider, environment: &str) -> VmResult<CodexState> {
    let output = provider
        .exec_output(
            Some(environment),
            &["sh".into(), "-c".into(), CODEX_PROBE.into()],
        )
        .map_err(VmError::from)?;
    parse_codex_state(&output)
}

fn parse_codex_state(output: &str) -> VmResult<CodexState> {
    let state = output
        .lines()
        .find_map(|line| line.trim().strip_prefix("VM_CODEX_STATE="))
        .unwrap_or_default();
    match state {
        "absent" => Ok(CodexState::Absent),
        "incomplete" => Ok(CodexState::Incomplete),
        "consumable" => Ok(CodexState::Consumable),
        value => Err(VmError::validation(
            format!("Codex runtime probe returned an unknown state '{value}'"),
            None::<String>,
        )),
    }
}

pub(super) fn codex_expected(config: &VmConfig) -> bool {
    config.preset.as_deref().is_some_and(|presets| {
        presets
            .split(',')
            .any(|preset| preset.trim().eq_ignore_ascii_case("vibe"))
    }) || config
        .vm
        .as_ref()
        .and_then(|settings| settings.get_box_spec())
        .is_some_and(|spec| match spec {
            BoxSpec::String(name) => name.to_ascii_lowercase().contains("vibe"),
            BoxSpec::Build { .. } => false,
        })
}

#[cfg(test)]
mod tests {
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
    use tempfile::TempDir;
    use vm_config::config::{BoxSpec, VmConfig, VmSettings};
    use vm_config::GlobalConfig;
    use vm_provider::{InstanceInfo, InstanceState, Provider, ProviderContext, VmStatusReport};

    use super::{
        codex_expected, parse_codex_state, reconcile_for, CodexState, CODEX_PROBE, CODEX_REPAIR,
    };

    #[derive(Clone)]
    struct FakeProvider {
        states: Arc<Mutex<VecDeque<&'static str>>>,
        calls: Arc<Mutex<Vec<&'static str>>>,
    }

    impl FakeProvider {
        fn new(states: impl IntoIterator<Item = &'static str>) -> Self {
            Self {
                states: Arc::new(Mutex::new(states.into_iter().collect())),
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Provider for FakeProvider {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn create(&self, _context: &ProviderContext) -> vm_core::error::Result<()> {
            Ok(())
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

        fn ssh(
            &self,
            _container: Option<&str>,
            _relative_path: &Path,
        ) -> vm_core::error::Result<()> {
            Ok(())
        }

        fn exec(
            &self,
            _container: Option<&str>,
            _command: &[String],
        ) -> vm_core::error::Result<()> {
            self.calls.lock().unwrap().push("repair");
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

        fn status(&self, _container: Option<&str>) -> vm_core::error::Result<VmStatusReport> {
            Ok(VmStatusReport::default())
        }

        fn instance_state(
            &self,
            _container: Option<&str>,
        ) -> vm_core::error::Result<InstanceState> {
            Ok(InstanceState::Running)
        }

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

        fn list_instances(&self) -> vm_core::error::Result<Vec<InstanceInfo>> {
            Ok(Vec::new())
        }

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
    fn repairs_only_vibe_or_existing_codex_runtimes() {
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
        for script in [CODEX_PROBE, CODEX_REPAIR] {
            assert!(Command::new("/bin/sh")
                .args(["-n", "-c", script])
                .status()
                .unwrap()
                .success());
        }
    }

    #[test]
    fn fake_provider_reconciles_fresh_state_then_becomes_idempotent() {
        let provider = FakeProvider::new(["absent", "consumable", "consumable"]);
        let config = VmConfig {
            preset: Some("vibe".into()),
            ..Default::default()
        };

        reconcile_for(&provider, "demo", &config, &GlobalConfig::default()).unwrap();
        reconcile_for(&provider, "demo", &config, &GlobalConfig::default()).unwrap();

        assert_eq!(
            provider.calls(),
            ["runtime", "probe", "repair", "probe", "runtime", "probe"]
        );
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
        assert_eq!(
            fs::read_link(prefix.join("bin/codex")).unwrap(),
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
        ] {
            assert!(fs::symlink_metadata(path).is_err());
        }
    }
}
