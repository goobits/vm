//! Detached, single-flight reconciliation for interactive shells.

use std::fs::{File, OpenOptions};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use fs2::FileExt;

use crate::commands::command_context::load_runtime_subject;
use crate::error::{VmError, VmResult};

use super::guest::InstallMode;
use super::{catalog, reconcile};

const SUCCESS_COOLDOWN: Duration = Duration::from_secs(60);
const RECEIPT: &[u8] = b"runtime-reconciliation-v1\n";

pub(in crate::commands) fn schedule(environment: &str) -> VmResult<()> {
    if cfg!(test) || std::env::var_os("VM_TEST_MODE").is_some() {
        return Ok(());
    }

    let paths = ReconcilePaths::discover(environment)?;
    if has_recent_receipt(&paths.success, SUCCESS_COOLDOWN) {
        return Ok(());
    }

    let executable = std::env::current_exe().map_err(VmError::from)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log)
        .map_err(VmError::from)?;
    Command::new("nohup")
        .arg(executable)
        .args(["tools", "reconcile-worker", environment])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(VmError::from)?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|error| {
            VmError::filesystem(
                error,
                paths.root.display().to_string(),
                "start guest reconciliation worker",
            )
        })?;
    Ok(())
}

pub(super) async fn run(environment: &str) -> VmResult<()> {
    let paths = ReconcilePaths::discover(environment)?;
    let lock = paths.open_lock()?;
    match lock.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
        Err(error) => return Err(VmError::from(error)),
    }
    if has_recent_receipt(&paths.success, SUCCESS_COOLDOWN) {
        return Ok(());
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&paths.log)
        .map_err(VmError::from)?;

    let subject = load_runtime_subject(None, None, Some(environment.to_string()))?;
    if !subject
        .provider
        .instance_state(Some(&subject.target))
        .map_err(VmError::from)?
        .is_running()
    {
        return Ok(());
    }
    reconcile::reconcile_environment(&subject)?;
    if !subject.config.tools.entries.is_empty() {
        catalog::prepare(std::slice::from_ref(&subject.config)).await?;
        reconcile::apply_updates(
            subject.provider.as_ref(),
            &subject.target,
            &subject.config,
            InstallMode::BackgroundIfIdle,
            false,
        )?;
    }
    vm_core::file_system::atomic_write(&paths.success, RECEIPT).map_err(VmError::from)
}

fn has_recent_receipt(path: &std::path::Path, cooldown: Duration) -> bool {
    std::fs::read(path).is_ok_and(|receipt| receipt == RECEIPT)
        && std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age < cooldown)
}

struct ReconcilePaths {
    root: std::path::PathBuf,
    lock: std::path::PathBuf,
    success: std::path::PathBuf,
    log: std::path::PathBuf,
}

impl ReconcilePaths {
    fn discover(environment: &str) -> VmResult<Self> {
        let digest = vm_packages::sha256_hex(environment);
        let root = vm_core::user_paths::vm_state_dir()?
            .join("runtime-reconciliation")
            .join(&digest[..24]);
        std::fs::create_dir_all(&root).map_err(VmError::from)?;
        Ok(Self::at(root))
    }

    fn at(root: std::path::PathBuf) -> Self {
        Self {
            lock: root.join("worker.lock"),
            success: root.join("last-success"),
            log: root.join("worker.log"),
            root,
        }
    }

    fn open_lock(&self) -> VmResult<File> {
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.lock)
            .map_err(VmError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipts_are_stable_and_cool_down_successful_work() {
        let directory = tempfile::tempdir().unwrap();
        let paths = ReconcilePaths::at(directory.path().join("demo"));
        std::fs::create_dir_all(&paths.root).unwrap();
        assert!(!has_recent_receipt(&paths.success, SUCCESS_COOLDOWN));
        std::fs::write(&paths.success, "old\n").unwrap();
        assert!(!has_recent_receipt(&paths.success, SUCCESS_COOLDOWN));
        std::fs::write(&paths.success, RECEIPT).unwrap();
        assert!(has_recent_receipt(&paths.success, SUCCESS_COOLDOWN));
        assert!(paths.lock.ends_with("worker.lock"));
        assert!(paths.log.ends_with("worker.log"));
    }

    #[test]
    fn worker_lock_is_single_flight() {
        let directory = tempfile::tempdir().unwrap();
        let paths = ReconcilePaths::at(directory.path().to_path_buf());
        let first = paths.open_lock().unwrap();
        let second = paths.open_lock().unwrap();
        first.try_lock_exclusive().unwrap();
        assert_eq!(
            second.try_lock_exclusive().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}
