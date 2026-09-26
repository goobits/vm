//! Freeze exact runtime identities and restore only states changed by this snapshot.

use crate::docker::{execute_docker_compose, execute_docker_with_output, ComposeProject};
use serde::Deserialize;
use vm_core::error::{Result, VmError};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerState {
    running: bool,
    paused: bool,
}

async fn state(executable: &str, id: &str) -> Result<ContainerState> {
    let output =
        execute_docker_with_output(executable, &["inspect", "--format", "{{json .State}}", id])
            .await?;
    serde_json::from_str(&output)
        .map_err(|error| VmError::general(error, format!("Cannot inspect container '{id}' state")))
}

pub(crate) async fn pause(executable: &str, compose: &ComposeProject) -> Result<Vec<String>> {
    let ids = execute_docker_compose(executable, &["ps", "--all", "-q"], compose).await?;
    let mut changed = Vec::new();
    let result = async {
        for id in ids.lines().filter(|id| !id.is_empty()) {
            let before = state(executable, id).await?;
            if !before.running || before.paused {
                continue;
            }
            // A runtime may transition before returning an error; include the
            // attempted identity in recovery, then inspect its actual state.
            changed.push(id.to_string());
            execute_docker_with_output(executable, &["pause", id]).await?;
            if !state(executable, id).await?.paused {
                return Err(VmError::Command(format!("Container '{id}' did not pause")));
            }
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(changed),
        Err(error) => finish(Err(error), resume(executable, &changed).await),
    }
}

pub(crate) async fn resume(executable: &str, ids: &[String]) -> Result<()> {
    let mut failures = Vec::new();
    for id in ids {
        let result = async {
            if state(executable, id).await?.paused {
                execute_docker_with_output(executable, &["unpause", id]).await?;
            }
            let after = state(executable, id).await?;
            if after.paused || !after.running {
                return Err(VmError::Command(format!(
                    "Container '{id}' did not return to running after snapshot capture"
                )));
            }
            Ok(())
        }
        .await;
        if let Err(error) = result {
            failures.push(error.to_string());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(VmError::Command(format!(
            "Could not resume snapshot containers: {}",
            failures.join("; ")
        )))
    }
}

pub(crate) fn finish<T>(snapshot: Result<T>, resume: Result<()>) -> Result<T> {
    match (snapshot, resume) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(snapshot_error), Err(resume_error)) => Err(VmError::general(
            resume_error,
            format!(
                "Snapshot failed ({snapshot_error}) and paused containers could not be resumed"
            ),
        )),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    fn fixture(root: &Path) -> (String, ComposeProject) {
        for (id, state) in [
            ("first", "running"),
            ("second", "running"),
            ("stopped", "stopped"),
            ("prior", "paused-running"),
            ("prior-podman", "paused"),
        ] {
            fs::write(root.join(id), state).unwrap();
        }
        let script = r#"#!/bin/sh
root=ROOT
case "$1" in
  compose) printf 'first\nsecond\nstopped\nprior\nprior-podman\n' ;;
  inspect)
    case "$(cat "$root/$4")" in
      running) printf '{"Running":true,"Paused":false}' ;;
      paused) printf '{"Running":false,"Paused":true}' ;;
      paused-running) printf '{"Running":true,"Paused":true}' ;;
      stopped) printf '{"Running":false,"Paused":false}' ;;
      *) exit 1 ;;
    esac ;;
  pause)
    printf paused > "$root/$2"
    if [ -f "$root/fail-pause" ] && [ "$2" = second ]; then echo 'partial pause failed' >&2; exit 1; fi ;;
  unpause)
    if [ "$2" = first ]; then
      if [ -f "$root/fail-unpause" ]; then echo 'unpause failed' >&2; exit 1; fi
      if [ -f "$root/noop-unpause" ]; then exit 0; fi
    fi
    printf running > "$root/$2" ;;
  *) exit 1 ;;
esac
"#;
        let runtime = root.join("runtime");
        let quoted = format!("'{}'", root.display().to_string().replace('\'', "'\"'\"'"));
        fs::write(&runtime, script.replace("ROOT", &quoted)).unwrap();
        fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
        (
            runtime.to_str().unwrap().to_string(),
            ComposeProject {
                directory: root.to_path_buf(),
                file: root.join("compose.yaml"),
            },
        )
    }

    fn assert_untouched(root: &Path) {
        assert_eq!(fs::read_to_string(root.join("stopped")).unwrap(), "stopped");
        assert_eq!(
            fs::read_to_string(root.join("prior")).unwrap(),
            "paused-running"
        );
        assert_eq!(
            fs::read_to_string(root.join("prior-podman")).unwrap(),
            "paused"
        );
    }

    #[tokio::test]
    async fn exact_id_recovery_preserves_initially_stopped_and_paused_containers() {
        let directory = tempfile::tempdir().unwrap();
        let (runtime, compose) = fixture(directory.path());
        let changed = pause(&runtime, &compose).await.unwrap();
        assert_eq!(changed, ["first", "second"]);
        assert_eq!(
            fs::read_to_string(directory.path().join("first")).unwrap(),
            "paused"
        );
        resume(&runtime, &changed).await.unwrap();
        assert_eq!(
            fs::read_to_string(directory.path().join("first")).unwrap(),
            "running"
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("second")).unwrap(),
            "running"
        );
        assert_untouched(directory.path());
    }

    #[tokio::test]
    async fn partial_pause_failure_recovers_every_changed_container() {
        let directory = tempfile::tempdir().unwrap();
        let (runtime, compose) = fixture(directory.path());
        fs::write(directory.path().join("fail-pause"), "").unwrap();
        let error = pause(&runtime, &compose).await.unwrap_err();
        assert!(error.to_string().contains("partial pause failed"));
        for id in ["first", "second"] {
            assert_eq!(
                fs::read_to_string(directory.path().join(id)).unwrap(),
                "running"
            );
        }
        assert_untouched(directory.path());
    }

    #[tokio::test]
    async fn resume_attempts_all_targets_and_detects_silent_noops() {
        for failure in ["fail-unpause", "noop-unpause"] {
            let directory = tempfile::tempdir().unwrap();
            let (runtime, compose) = fixture(directory.path());
            let changed = pause(&runtime, &compose).await.unwrap();
            fs::write(directory.path().join(failure), "").unwrap();
            assert!(resume(&runtime, &changed).await.is_err());
            assert_eq!(
                fs::read_to_string(directory.path().join("first")).unwrap(),
                "paused"
            );
            assert_eq!(
                fs::read_to_string(directory.path().join("second")).unwrap(),
                "running"
            );
            assert_untouched(directory.path());
        }
    }
}
