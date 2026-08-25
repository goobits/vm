use std::fs::{File, OpenOptions};
use std::process::{Command, Stdio};
use std::time::Duration;

use fs2::FileExt;
use vm_config::GlobalConfig;

use crate::error::{VmError, VmResult};

use super::{rollout, user_service};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub(in crate::commands) fn ensure_worker() -> VmResult<()> {
    if GlobalConfig::load()?.tools.is_empty() || std::env::var("VM_TEST_MODE").is_ok() {
        return Ok(());
    }
    let paths = WorkerPaths::discover()?;
    let executable = std::env::current_exe().map_err(VmError::from)?;
    if std::env::var_os("VM_PACKAGES_COMPOSE_PROJECT").is_none()
        && user_service::install(&executable)?
    {
        return Ok(());
    }
    let lock = paths.open_lock()?;
    match lock.try_lock_exclusive() {
        Ok(()) => FileExt::unlock(&lock).map_err(VmError::from)?,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
        Err(error) => return Err(VmError::from(error)),
    }

    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.log)
        .map_err(VmError::from)?;
    Command::new("nohup")
        .arg(executable)
        .args(["tools", "activation-worker"])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(VmError::from)?))
        .stderr(Stdio::from(log))
        .spawn()
        .map_err(|error| {
            VmError::filesystem(
                error,
                paths.root.display().to_string(),
                "start tool activation worker",
            )
        })?;
    Ok(())
}

pub(in crate::commands) fn remove_worker() -> VmResult<()> {
    let paths = WorkerPaths::discover()?;
    user_service::remove()?;
    let lock = paths.open_lock()?;
    match lock.try_lock_exclusive() {
        Ok(()) => FileExt::unlock(&lock).map_err(VmError::from)?,
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            let pid = std::fs::read_to_string(&paths.pid)
                .ok()
                .and_then(|pid| pid.trim().parse::<u32>().ok());
            if let Some(pid) = pid {
                let _ = Command::new("kill").arg(pid.to_string()).status();
            }
        }
        Err(error) => return Err(VmError::from(error)),
    }
    match std::fs::remove_file(&paths.pid) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(VmError::from(error)),
    }
    Ok(())
}

pub(in crate::commands) async fn run_worker(once: bool) -> VmResult<()> {
    let paths = WorkerPaths::discover()?;
    let lock = paths.open_lock()?;
    match lock.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
        Err(error) => return Err(VmError::from(error)),
    }
    std::fs::write(&paths.pid, format!("{}\n", std::process::id())).map_err(VmError::from)?;
    let _pid = WorkerPid { path: paths.pid };

    loop {
        let result = rollout::process_next().await;
        if once {
            return result.map(|_| ());
        }
        match result {
            Ok(processed) => {
                if !processed {
                    tokio::time::sleep(POLL_INTERVAL).await;
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Tool activation worker will retry");
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
    }
}

pub(super) fn worker_id() -> VmResult<String> {
    let home = vm_core::user_paths::home_dir()?;
    let host = std::env::var("HOSTNAME").unwrap_or_else(|_| "controller".into());
    let digest = vm_packages::sha256_hex(format!("{}\0{}", home.display(), host));
    Ok(format!("host-{}", &digest[..24]))
}

struct WorkerPaths {
    root: std::path::PathBuf,
    lock: std::path::PathBuf,
    pid: std::path::PathBuf,
    log: std::path::PathBuf,
}

impl WorkerPaths {
    fn discover() -> VmResult<Self> {
        let root = vm_core::user_paths::vm_state_dir()?
            .join("infrastructure")
            .join("packages");
        std::fs::create_dir_all(&root).map_err(VmError::from)?;
        Ok(Self::at(root))
    }

    fn at(root: std::path::PathBuf) -> Self {
        Self {
            lock: root.join("activation-worker.lock"),
            pid: root.join("activation-worker.pid"),
            log: root.join("activation-worker.log"),
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

struct WorkerPid {
    path: std::path::PathBuf,
}

impl Drop for WorkerPid {
    fn drop(&mut self) {
        let current = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|pid| pid.trim().parse::<u32>().ok());
        if current == Some(std::process::id()) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_paths_are_stable_managed_components() {
        let paths = WorkerPaths::at("/tmp/vm-packages".into());
        assert!(paths.lock.ends_with("activation-worker.lock"));
        assert!(paths.pid.ends_with("activation-worker.pid"));
        assert!(paths.log.ends_with("activation-worker.log"));
        assert!(vm_packages::validate_managed_id("worker", &worker_id().unwrap()).is_ok());
    }

    #[test]
    fn pid_guard_removes_only_its_own_receipt() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("worker.pid");
        std::fs::write(&path, std::process::id().to_string()).unwrap();
        drop(WorkerPid { path: path.clone() });
        assert!(!path.exists());

        std::fs::write(&path, "1").unwrap();
        drop(WorkerPid { path: path.clone() });
        assert!(path.exists());
    }
}
