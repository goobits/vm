//! Provider-native Tart VM snapshots. Tart exports a stopped VM as one .tvm image.

use crate::archive::{directory_size, file_digest, validate_snapshot_files};
use crate::manager::{snapshot_file_path, SnapshotManager, SnapshotScope};
use crate::metadata::{ExcludedMount, SnapshotMetadata};
use chrono::Utc;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

const NATIVE_FILE: &str = "vm.tvm";

#[derive(Deserialize)]
struct TartEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Source")]
    source: String,
}

struct TartRuntime<'a> {
    program: &'a Path,
    home: Option<&'a Path>,
}

impl TartRuntime<'_> {
    fn command(&self) -> Command {
        let mut command = Command::new(self.program);
        if let Some(home) = self.home {
            command.env("TART_HOME", home);
        }
        command
    }

    fn run(&self, args: &[&str]) -> Result<()> {
        let output = self.command().args(args).output().map_err(|error| {
            VmError::general(error, format!("Failed to execute tart {}", args.join(" ")))
        })?;
        if !output.status.success() {
            return Err(VmError::validation(
                format!(
                    "tart {} failed: {}",
                    args.join(" "),
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
                None::<String>,
            ));
        }
        Ok(())
    }

    fn entries(&self) -> Result<Vec<TartEntry>> {
        let output = self.command().args(["list", "--format", "json"]).output()?;
        if !output.status.success() {
            return Err(VmError::validation(
                format!(
                    "tart list failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
                None::<String>,
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| VmError::general(error, "Invalid Tart VM inventory"))
    }

    fn require_stopped_local(&self, name: &str) -> Result<()> {
        let entry = self
            .entries()?
            .into_iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| {
                VmError::validation(format!("Tart VM '{name}' does not exist"), None::<String>)
            })?;
        if entry.source != "local" || !entry.state.eq_ignore_ascii_case("stopped") {
            return Err(VmError::validation(
                format!("Tart VM '{name}' must be a stopped local VM for a consistent snapshot"),
                Some("Stop the environment before creating or restoring a snapshot"),
            ));
        }
        Ok(())
    }

    fn restore_archive(&self, archive: &Path, target_environment: &str) -> Result<()> {
        self.require_stopped_local(target_environment)?;
        let archive_arg = archive.to_str().ok_or_else(|| {
            VmError::validation("Snapshot path is not valid UTF-8 for Tart", None::<String>)
        })?;
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let staged = format!("vm-restore-{nonce}");
        let backup = format!("vm-previous-{nonce}");
        self.run(&["import", archive_arg, &staged])?;
        if let Err(error) = self.run(&["rename", target_environment, &backup]) {
            let _ = self.run(&["delete", &staged]);
            return Err(error);
        }
        if let Err(error) = self.run(&["rename", &staged, target_environment]) {
            let recovery = self.run(&["rename", &backup, target_environment]);
            let _ = self.run(&["delete", &staged]);
            return match recovery {
                Ok(()) => Err(error),
                Err(recovery_error) => Err(VmError::general(
                    recovery_error,
                    format!("Restore failed ({error}); original VM remains named '{backup}'"),
                )),
            };
        }
        self.run(&["delete", &backup]).map_err(|error| {
            VmError::general(error, format!("Restore installed '{target_environment}', but the previous VM remains named '{backup}'"))
        })
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    config: &VmConfig,
    name: &str,
    description: Option<&str>,
    quiesce: bool,
    project: &str,
    source_environment: &str,
    project_dir: &Path,
    home: Option<&Path>,
) -> Result<()> {
    if quiesce {
        return Err(VmError::validation(
            "Tart cannot quiesce a running guest for this snapshot format",
            Some("Stop the environment, then create the snapshot without --quiesce"),
        ));
    }
    let manager = SnapshotManager::new()?;
    let owner = config.owning_config_path().ok_or_else(|| {
        VmError::validation(
            "Tart snapshot creation requires a project configuration",
            None::<String>,
        )
    })?;
    let scope = SnapshotScope::OwnedProject {
        name: project,
        config_path: owner,
    };
    if manager.snapshot_exists(scope, name)? {
        return Err(VmError::validation(
            format!("Snapshot '{name}' already exists in project '{project}'"),
            None::<String>,
        ));
    }
    let runtime = TartRuntime {
        program: Path::new("tart"),
        home,
    };
    runtime.require_stopped_local(source_environment)?;
    let staging = manager.create_staging_dir(scope, name)?;
    let native_dir = staging.path().join("native");
    std::fs::create_dir(&native_dir)?;
    let archive = native_dir.join(NATIVE_FILE);
    let archive_arg = archive.to_str().ok_or_else(|| {
        VmError::validation("Snapshot path is not valid UTF-8 for Tart", None::<String>)
    })?;
    runtime.run(&["export", source_environment, archive_arg])?;
    if !archive.is_file() || std::fs::metadata(&archive)?.len() == 0 {
        return Err(VmError::validation(
            "Tart export did not create a nonempty VM archive",
            None::<String>,
        ));
    }
    let excluded_mounts = config
        .mounts
        .iter()
        .map(|mount| ExcludedMount {
            service: source_environment.to_string(),
            kind: "host_share".to_string(),
            source: Some(mount.source.display().to_string()),
            target: mount.target.display().to_string(),
        })
        .chain(std::iter::once(ExcludedMount {
            service: source_environment.to_string(),
            kind: "host_share".to_string(),
            source: Some(project_dir.display().to_string()),
            target: "workspace".to_string(),
        }))
        .collect();
    let metadata = SnapshotMetadata {
        name: name.to_string(),
        created_at: Utc::now(),
        description: description.map(str::to_string),
        project_name: project.to_string(),
        source_environment: Some(source_environment.to_string()),
        provider: "tart".to_string(),
        architecture: vm_platform::platform::architecture().to_string(),
        consistency: "stopped".to_string(),
        project_dir: project_dir.display().to_string(),
        owner_config_path: Some(owner.canonicalize()?.display().to_string()),
        git_commit: None,
        git_dirty: false,
        git_branch: None,
        services: vec![],
        volumes: vec![],
        native_vm_file: Some(NATIVE_FILE.to_string()),
        native_image_digest: Some(format!("sha256:{}", file_digest(&archive)?)),
        excluded_mounts,
        compose_file: String::new(),
        vm_config_file: String::new(),
        total_size_bytes: directory_size(staging.path())?,
    };
    metadata.save(staging.path().join("metadata.json"))?;
    manager.install_staged_snapshot(staging, scope, name, false)
}

pub async fn restore(
    name: &str,
    project: &str,
    target_environment: &str,
    home: Option<&Path>,
    owner: &Path,
) -> Result<()> {
    let manager = SnapshotManager::new()?;
    let snapshot_dir = manager.get_snapshot_dir(
        SnapshotScope::OwnedProject {
            name: project,
            config_path: owner,
        },
        name,
    )?;
    let metadata = SnapshotMetadata::load(snapshot_dir.join("metadata.json"))?;
    if metadata.provider != "tart"
        || metadata.project_name != project
        || metadata.owner_config_path.as_deref()
            != Some(owner.canonicalize()?.to_string_lossy().as_ref())
    {
        return Err(VmError::validation(
            "Snapshot provider or project does not match the selected Tart environment",
            None::<String>,
        ));
    }
    let native_file = metadata
        .native_vm_file
        .as_deref()
        .ok_or_else(|| VmError::validation("Snapshot has no Tart VM archive", None::<String>))?;
    if metadata.source_environment.as_deref() != Some(target_environment) {
        return Err(VmError::validation(
            format!("Snapshot '{name}' was captured from a different environment"),
            None::<String>,
        ));
    }
    if metadata.architecture != vm_platform::platform::architecture() {
        return Err(VmError::validation(
            "Snapshot architecture does not match this host",
            None::<String>,
        ));
    }
    validate_snapshot_files(&snapshot_dir, &metadata)?;
    let runtime = TartRuntime {
        program: Path::new("tart"),
        home,
    };
    let archive = snapshot_file_path(
        &snapshot_dir.join("native"),
        native_file,
        "native VM archive",
    )?;
    runtime.restore_archive(&archive, target_environment)
}

#[cfg(all(test, unix))]
mod tests {
    use super::TartRuntime;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn restore_stages_replacement_and_recovers_original_on_install_failure() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        std::fs::create_dir_all(home.join("vms")).unwrap();
        let script = root.path().join("tart");
        std::fs::write(
            &script,
            r#"#!/bin/sh
set -eu
case "$1" in
  list) printf '[{"Name":"demo-dev","State":"stopped","Source":"local"}]' ;;
  import) cp "$2" "$TART_HOME/vms/$3" ;;
  rename)
    case "$2" in
      vm-restore-*)
        if [ -f "$TART_HOME/fail-install" ]; then exit 19; fi ;;
    esac
    mv "$TART_HOME/vms/$2" "$TART_HOME/vms/$3" ;;
  delete) rm "$TART_HOME/vms/$2" ;;
  *) exit 20 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let archive = root.path().join("vm.tvm");
        std::fs::write(&archive, b"captured").unwrap();
        let target = home.join("vms/demo-dev");
        std::fs::write(&target, b"original").unwrap();
        let runtime = TartRuntime {
            program: &script,
            home: Some(&home),
        };

        runtime.restore_archive(&archive, "demo-dev").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"captured");
        assert_eq!(std::fs::read_dir(home.join("vms")).unwrap().count(), 1);

        std::fs::write(&target, b"original").unwrap();
        std::fs::write(home.join("fail-install"), b"").unwrap();
        assert!(runtime.restore_archive(&archive, "demo-dev").is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"original");
        assert_eq!(std::fs::read_dir(home.join("vms")).unwrap().count(), 1);
    }
}
