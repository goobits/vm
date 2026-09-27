//! Detached, single-flight reconciliation for interactive shells.

use std::fs::{File, OpenOptions};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::cli::FleetArgs;
use crate::commands::command_context::{load_runtime_subject_for_instance, RuntimeSubject};
use crate::commands::vm_ops::{self, InstanceStateFilter};
use crate::error::{VmError, VmResult};
use vm_provider::InstanceInfo;

use super::guest::InstallMode;
use super::{catalog, reconcile};

const SUCCESS_COOLDOWN: Duration = Duration::from_secs(60);
const RECEIPT: &[u8] = b"runtime-reconciliation-v1\n";

/// An explicit update request waiting for an already-stopped environment to run.
/// Current configuration is checked again before applying it, so disabling a
/// tool never re-enables it through a stale intent.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct PendingUpdate {
    pub managed: std::collections::BTreeSet<String>,
    pub vendors: std::collections::BTreeSet<String>,
    pub all_managed: bool,
    pub all_vendors: bool,
}

impl PendingUpdate {
    fn merge(&mut self, other: Self) {
        self.managed.extend(other.managed);
        self.vendors.extend(other.vendors);
        self.all_managed |= other.all_managed;
        self.all_vendors |= other.all_vendors;
    }
}

pub(super) fn defer(provider: &str, environment: &str, update: PendingUpdate) -> VmResult<()> {
    let paths = ReconcilePaths::discover(environment)?;
    let lock = paths.open_lock()?;
    lock.lock_exclusive().map_err(VmError::from)?;
    let mut pending = read_pending(&paths, provider)?.unwrap_or_default();
    pending.merge(update);
    write_pending(&paths, provider, &pending)?;
    match std::fs::remove_file(&paths.success) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VmError::from(error)),
    }
}

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
        Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
            return Ok(())
        }
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

    let instance = running_instance(environment)?;
    let subject = load_runtime_subject_for_instance(None, None, &instance)?;
    if !subject
        .provider
        .instance_state(Some(&subject.target))
        .map_err(VmError::from)?
        .is_running()
    {
        return Ok(());
    }
    if let Some(pending) = read_pending(&paths, subject.provider.name())? {
        apply_pending(&subject, pending, InstallMode::BackgroundIfIdle).await?;
        std::fs::remove_file(paths.pending_for(subject.provider.name())).map_err(VmError::from)?;
    } else {
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
    }
    vm_core::file_system::atomic_write(&paths.success, RECEIPT).map_err(VmError::from)
}

fn running_instance(environment: &str) -> VmResult<InstanceInfo> {
    let matches = vm_ops::resolve_fleet_targets(
        &FleetArgs {
            fleet: true,
            provider: None,
            pattern: None,
        },
        InstanceStateFilter::Running,
    )?
    .into_iter()
    .filter(|instance| instance.name == environment)
    .collect::<Vec<_>>();
    match matches.as_slice() {
        [instance] => Ok(instance.clone()),
        [] => Err(VmError::validation(
            format!("Running environment '{environment}' was not found"),
            None::<String>,
        )),
        _ => Err(VmError::validation(
            format!("Environment name '{environment}' is ambiguous across providers"),
            Some("Use `vm tools update --env NAME` with an exact project target"),
        )),
    }
}

pub(super) async fn apply_deferred_after_start(provider: &str, environment: &str) -> VmResult<()> {
    let paths = ReconcilePaths::discover(environment)?;
    if !paths.pending_for(provider).exists() {
        return Ok(());
    }
    let lock = paths.open_lock()?;
    lock.lock_exclusive().map_err(VmError::from)?;
    let Some(pending) = read_pending(&paths, provider)? else {
        return Ok(());
    };
    let instance = InstanceInfo {
        name: environment.to_string(),
        id: String::new(),
        status: "running".into(),
        provider: provider.to_string(),
        project: None,
        uptime: None,
        created_at: None,
    };
    let subject = load_runtime_subject_for_instance(None, None, &instance)?;
    if !subject
        .provider
        .instance_state(Some(environment))
        .map_err(VmError::from)?
        .is_running()
    {
        return Ok(());
    }
    apply_pending(&subject, pending, InstallMode::Wait).await?;
    std::fs::remove_file(paths.pending_for(provider)).map_err(VmError::from)?;
    vm_core::file_system::atomic_write(&paths.success, RECEIPT).map_err(VmError::from)
}

async fn apply_pending(
    subject: &RuntimeSubject,
    pending: PendingUpdate,
    mode: InstallMode,
) -> VmResult<()> {
    let selected = selected_configuration(&subject.config, &pending);
    reconcile::reconcile_environment(subject)?;
    if pending.all_vendors || !pending.vendors.is_empty() {
        let names = pending.vendors.into_iter().collect::<Vec<_>>();
        crate::commands::base::update_vendor_tools(
            subject.provider.as_ref(),
            &subject.target,
            &selected,
            &names,
            pending.all_vendors,
            mode != InstallMode::Wait,
        )?;
    }
    if (pending.all_managed || !pending.managed.is_empty()) && !selected.tools.entries.is_empty() {
        catalog::prepare(std::slice::from_ref(&selected)).await?;
        reconcile::apply_updates(
            subject.provider.as_ref(),
            &subject.target,
            &selected,
            mode,
            !pending.all_managed,
        )?;
    }
    Ok(())
}

fn selected_configuration(
    config: &vm_config::config::VmConfig,
    pending: &PendingUpdate,
) -> vm_config::config::VmConfig {
    let mut selected = config.clone();
    if !pending.all_managed {
        selected
            .tools
            .entries
            .retain(|name, _| pending.managed.contains(name));
    }
    selected
}

fn read_pending(paths: &ReconcilePaths, provider: &str) -> VmResult<Option<PendingUpdate>> {
    match std::fs::read(paths.pending_for(provider)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| {
            VmError::validation(
                format!("Invalid deferred tool update: {error}"),
                None::<String>,
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(VmError::from(error)),
    }
}

fn write_pending(paths: &ReconcilePaths, provider: &str, pending: &PendingUpdate) -> VmResult<()> {
    let bytes = serde_json::to_vec(pending).map_err(VmError::from)?;
    vm_core::file_system::atomic_write(&paths.pending_for(provider), &bytes).map_err(VmError::from)
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

    fn pending_for(&self, provider: &str) -> std::path::PathBuf {
        let digest = vm_packages::sha256_hex(provider);
        self.root
            .join(format!("pending-tools-{}.json", &digest[..16]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vm_config::config::{ToolConfig, VmConfig};

    #[test]
    fn deferred_selection_is_durable_and_uses_current_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let paths = ReconcilePaths::at(directory.path().to_path_buf());
        let mut first = PendingUpdate {
            managed: ["typemill".into()].into(),
            ..Default::default()
        };
        write_pending(&paths, "docker", &first).unwrap();
        let second = PendingUpdate {
            managed: ["codeatlas".into()].into(),
            ..Default::default()
        };
        first.merge(second);
        write_pending(&paths, "docker", &first).unwrap();
        let restored = read_pending(&paths, "docker").unwrap().unwrap();
        assert!(read_pending(&paths, "tart").unwrap().is_none());
        assert_eq!(
            restored.managed,
            ["codeatlas".into(), "typemill".into()].into()
        );

        let mut current = VmConfig::default();
        current
            .tools
            .entries
            .insert("codeatlas".into(), ToolConfig::default());
        current
            .tools
            .entries
            .insert("other".into(), ToolConfig::default());
        let selected = selected_configuration(&current, &restored);
        assert!(selected.tools.entries.contains_key("codeatlas"));
        assert!(!selected.tools.entries.contains_key("typemill"));
        assert!(!selected.tools.entries.contains_key("other"));
    }

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
            second.try_lock_exclusive().unwrap_err().raw_os_error(),
            fs2::lock_contended_error().raw_os_error()
        );
    }
}
