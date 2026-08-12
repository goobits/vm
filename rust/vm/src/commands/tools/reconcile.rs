use vm_config::{config::BoxSpec, config::VmConfig, GlobalConfig};
use vm_core::{vm_progress, vm_success};
use vm_provider::{Provider, ProviderContext};

use crate::error::{VmError, VmResult};

use super::super::command_context::RuntimeSubject;

const CODEX_PROBE: &str = r#"
codex_path="$(command -v codex 2>/dev/null || true)"
if test -z "$codex_path"; then
  printf '%s\n' VM_CODEX_STATE=absent
  exit 0
fi
if test ! -x "$codex_path"; then
  printf '%s\n' VM_CODEX_STATE=incomplete
  exit 0
fi
resolved="$(readlink -f "$codex_path" 2>/dev/null || printf '%s' "$codex_path")"
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

as_root() {
  if test "$(id -u)" -eq 0; then
    "$@"
  elif command -v sudo >/dev/null 2>&1; then
    sudo -n "$@"
  else
    printf '%s\n' 'Codex repair requires root or passwordless sudo' >&2
    return 1
  fi
}

temporary="$(mktemp -d "${TMPDIR:-/tmp}/vm-codex-reconcile.XXXXXX")"
stage=""
backup=""
rollback_needed=no
cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  if test "$rollback_needed" = yes; then
    as_root rm -rf /usr/local/lib/vm-ai-tools/codex-package >/dev/null 2>&1 || true
    if test -n "$backup"; then
      as_root mv "$backup" /usr/local/lib/vm-ai-tools/codex-package \
        >/dev/null 2>&1 || true
    fi
  fi
  rm -rf "$temporary"
  if test -n "$stage"; then
    as_root rm -rf "$stage" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

export HOME="$temporary/home"
mkdir -p "$HOME/.local/bin"
export PATH="$HOME/.local/bin:$PATH"
installer="$temporary/install-codex.sh"
curl --fail --silent --show-error --location \
  --connect-timeout 10 --max-time 600 --retry 2 \
  --output "$installer" https://chatgpt.com/codex/install.sh
sh "$installer"
hash -r

codex_path="$(command -v codex)"
resolved="$(readlink -f "$codex_path")"
bin_dir="$(dirname "$resolved")"
package_source="$(dirname "$bin_dir")"
test -f "$package_source/codex-package.json"
test -x "$bin_dir/codex-code-mode-host"

root=/usr/local/lib/vm-ai-tools
target="$root/codex-package"
as_root install -d -m 0755 "$root"
stage="$(as_root mktemp -d "$root/.codex-stage.XXXXXX")"
as_root cp -R "$package_source/." "$stage/"
as_root test -f "$stage/codex-package.json"
as_root test -x "$stage/bin/codex"
as_root test -x "$stage/bin/codex-code-mode-host"

if as_root test -e "$target"; then
  backup="$root/.codex-previous.$$"
  as_root mv "$target" "$backup"
fi
rollback_needed=yes
as_root mv "$stage" "$target"
stage=""

as_root ln -sfn "$target/bin/codex" "$root/codex"
as_root ln -sfn "$target/bin/codex-code-mode-host" "$root/codex-code-mode-host"
as_root ln -sfn "$root/codex" /usr/local/bin/codex
as_root ln -sfn "$root/codex-code-mode-host" /usr/local/bin/codex-code-mode-host
test -x /usr/local/bin/codex-code-mode-host
/usr/local/bin/codex --version >/dev/null
rollback_needed=no
if test -n "$backup"; then
  as_root rm -rf "$backup"
  backup=""
fi
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
    use std::path::Path;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    use vm_config::config::{BoxSpec, VmConfig, VmSettings};
    use vm_config::GlobalConfig;
    use vm_provider::{InstanceInfo, InstanceState, Provider, ProviderContext, VmStatusReport};

    use super::{codex_expected, parse_codex_state, reconcile_for, CodexState, CODEX_REPAIR};

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
        assert!(Command::new("/bin/sh")
            .args(["-n", "-c", CODEX_REPAIR])
            .status()
            .unwrap()
            .success());
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
}
