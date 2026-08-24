use std::fs::{File, OpenOptions};
use std::process::{Command, Stdio};
use std::time::Duration;

use fs2::FileExt;
use vm_config::GlobalConfig;
use vm_core::vm_warning;
use vm_packages::{
    ClaimToolActivationRequest, FinishToolActivationRequest, PlanToolActivationRequest,
    ToolActivationRecord, ToolActivationTargetPlan, ToolActivationTargetState,
    UpdateToolActivationTargetRequest,
};
use vm_provider::InstanceInfo;

use crate::cli::FleetArgs;
use crate::commands::command_context::load_runtime_subject_for_instance;
use crate::commands::packages::tooling;
use crate::commands::vm_ops::{self, InstanceStateFilter};
use crate::error::{VmError, VmResult};

use super::{reconcile_subject, updates};

const WORKER_POLL_INTERVAL: Duration = Duration::from_secs(2);
const TARGET_RETRY_INTERVAL: Duration = Duration::from_secs(2);
const TARGET_RETRY_TIMEOUT: Duration = Duration::from_secs(2 * 60);
const WORKER_LEASE_SECONDS: u64 = 5 * 60;
#[cfg(any(test, target_os = "macos"))]
const WORKER_COMMAND_PATH: &str =
    "/opt/homebrew/bin:/opt/homebrew/sbin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin";

pub(in crate::commands) fn ensure_worker() -> VmResult<()> {
    if GlobalConfig::load()?.tools.is_empty() || std::env::var("VM_TEST_MODE").is_ok() {
        return Ok(());
    }
    let paths = WorkerPaths::discover()?;
    let executable = std::env::current_exe().map_err(VmError::from)?;
    // A compose-project override identifies an isolated controller. Keep its
    // worker in the invoking environment instead of leaking that test/dev
    // override into the user's persistent service manager.
    if std::env::var_os("VM_PACKAGES_COMPOSE_PROJECT").is_none()
        && install_user_service(&executable)?
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
    remove_user_service()?;
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

#[cfg(target_os = "linux")]
fn install_user_service(executable: &std::path::Path) -> VmResult<bool> {
    let directory = vm_core::user_paths::home_dir()?.join(".config/systemd/user");
    std::fs::create_dir_all(&directory).map_err(VmError::from)?;
    let path = directory.join("vm-tool-activation.service");
    let executable = executable.to_string_lossy().replace('%', "%%");
    let content = format!(
        "[Unit]\nDescription=VM managed-tool activation\n\n[Service]\nType=simple\nExecStart={executable:?} tools activation-worker\nRestart=always\nRestartSec=2\n\n[Install]\nWantedBy=default.target\n"
    );
    write_if_changed(&path, content.as_bytes())?;
    let reloaded = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if !reloaded.is_ok_and(|status| status.success()) {
        return Ok(false);
    }
    let enabled = Command::new("systemctl")
        .args(["--user", "enable", "--now", "vm-tool-activation.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !enabled {
        return Ok(false);
    }
    Ok(Command::new("systemctl")
        .args(["--user", "restart", "vm-tool-activation.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success()))
}

#[cfg(target_os = "macos")]
fn install_user_service(executable: &std::path::Path) -> VmResult<bool> {
    let directory = vm_core::user_paths::home_dir()?.join("Library/LaunchAgents");
    std::fs::create_dir_all(&directory).map_err(VmError::from)?;
    let path = directory.join("com.goobits.vm-tool-activation.plist");
    let executable = xml_escape(&executable.to_string_lossy());
    let content = launchd_service(&executable);
    let changed = write_if_changed(&path, content.as_bytes())?;
    let domain = launchd_domain()?;
    let label = format!("{domain}/com.goobits.vm-tool-activation");
    if changed {
        let _ = Command::new("launchctl")
            .args(["bootout", &label])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    if !Command::new("launchctl")
        .args(["print", &label])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
    {
        let status = Command::new("launchctl")
            .arg("bootstrap")
            .arg(&domain)
            .arg(&path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if !status.is_ok_and(|status| status.success()) {
            return Ok(false);
        }
    }
    Ok(Command::new("launchctl")
        .args(["kickstart", "-k", &label])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success()))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn install_user_service(_executable: &std::path::Path) -> VmResult<bool> {
    Ok(false)
}

#[cfg(target_os = "linux")]
fn remove_user_service() -> VmResult<()> {
    let path =
        vm_core::user_paths::home_dir()?.join(".config/systemd/user/vm-tool-activation.service");
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "vm-tool-activation.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    remove_if_present(&path)?;
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    Ok(())
}

#[cfg(target_os = "macos")]
fn remove_user_service() -> VmResult<()> {
    let path = vm_core::user_paths::home_dir()?
        .join("Library/LaunchAgents/com.goobits.vm-tool-activation.plist");
    if let Ok(domain) = launchd_domain() {
        let _ = Command::new("launchctl")
            .args([
                "bootout",
                &format!("{domain}/com.goobits.vm-tool-activation"),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    remove_if_present(&path)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn remove_user_service() -> VmResult<()> {
    Ok(())
}

fn write_if_changed(path: &std::path::Path, content: &[u8]) -> VmResult<bool> {
    if std::fs::read(path).is_ok_and(|current| current == content) {
        return Ok(false);
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary, content).map_err(VmError::from)?;
    std::fs::rename(&temporary, path).map_err(VmError::from)?;
    Ok(true)
}

fn remove_if_present(path: &std::path::Path) -> VmResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VmError::from(error)),
    }
}

#[cfg(target_os = "macos")]
fn launchd_domain() -> VmResult<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(VmError::from)?;
    let uid = std::str::from_utf8(&output.stdout)
        .map_err(|_| {
            VmError::validation("Could not resolve the launchd user domain", None::<String>)
        })?
        .trim();
    if !output.status.success()
        || uid.is_empty()
        || !uid.chars().all(|character| character.is_ascii_digit())
    {
        return Err(VmError::validation(
            "Could not resolve the launchd user domain",
            None::<String>,
        ));
    }
    Ok(format!("gui/{uid}"))
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(any(test, target_os = "macos"))]
fn launchd_service(executable: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n<key>Label</key><string>com.goobits.vm-tool-activation</string>\n<key>ProgramArguments</key><array><string>{executable}</string><string>tools</string><string>activation-worker</string></array>\n<key>EnvironmentVariables</key><dict><key>PATH</key><string>{WORKER_COMMAND_PATH}</string></dict>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n<key>ProcessType</key><string>Background</string>\n</dict></plist>\n"
    )
}

pub(super) async fn run_worker(once: bool) -> VmResult<()> {
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
        let result = process_next().await;
        if once {
            return result.map(|_| ());
        }
        match result {
            Ok(processed) => {
                if !processed {
                    tokio::time::sleep(WORKER_POLL_INTERVAL).await;
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Tool activation worker will retry");
                tokio::time::sleep(WORKER_POLL_INTERVAL).await;
            }
        }
    }
}

pub(in crate::commands) async fn repair() -> VmResult<usize> {
    ensure_worker()?;
    let client = tooling::client()?;
    let repaired = client.repair_tool_activations().await?;
    reconcile_running_environments().await?;
    Ok(repaired)
}

pub(in crate::commands) async fn activate_deferred(
    provider: &str,
    environment: &str,
) -> VmResult<()> {
    let client = tooling::client()?;
    let activations = client.tool_activations().await?;
    for activation in activations {
        let Some(target) = activation.targets.iter().find(|target| {
            target.provider == provider
                && target.environment == environment
                && target.state == ToolActivationTargetState::Deferred
        }) else {
            continue;
        };
        let worker = worker_id()?;
        let Some(claimed) = client
            .claim_tool_activation(
                &activation.activation_id,
                &ClaimToolActivationRequest {
                    worker: worker.clone(),
                    lease_seconds: WORKER_LEASE_SECONDS,
                },
            )
            .await?
        else {
            continue;
        };
        activate_target(&client, &claimed, target.target_id.as_str(), &worker).await?;
        let finished = finish(&client, &claimed, &worker).await?;
        if let Some(failed) = finished.targets.iter().find(|candidate| {
            candidate.target_id == target.target_id
                && candidate.state == ToolActivationTargetState::Failed
        }) {
            return Err(VmError::validation(
                format!(
                    "Tool '{}' activation failed in '{}': {}",
                    finished.tool,
                    environment,
                    failed.error.as_deref().unwrap_or("unknown error")
                ),
                Some("Run `vm packages doctor --fix` on the controller and retry the start"),
            ));
        }
    }
    Ok(())
}

async fn process_next() -> VmResult<bool> {
    let worker = worker_id()?;
    let client = tooling::client()?;
    let Some(mut activation) = client
        .claim_next_tool_activation(&ClaimToolActivationRequest {
            worker: worker.clone(),
            lease_seconds: WORKER_LEASE_SECONDS,
        })
        .await?
    else {
        return Ok(false);
    };
    if activation.targets.is_empty() {
        activation = plan(&client, activation, &worker).await?;
    }
    let pending = activation
        .targets
        .iter()
        .filter(|target| target.state == ToolActivationTargetState::Pending)
        .map(|target| target.target_id.clone())
        .collect::<Vec<_>>();
    for target_id in pending {
        let Some(renewed) = client
            .claim_tool_activation(
                &activation.activation_id,
                &ClaimToolActivationRequest {
                    worker: worker.clone(),
                    lease_seconds: WORKER_LEASE_SECONDS,
                },
            )
            .await?
        else {
            return Err(VmError::validation(
                "Tool activation lease was lost",
                Some("The host worker will retry after the current lease expires"),
            ));
        };
        activation = renewed;
        activate_target(&client, &activation, &target_id, &worker).await?;
    }
    finish(&client, &activation, &worker).await?;
    Ok(true)
}

async fn plan(
    client: &vm_packages::PackageInfrastructureClient,
    activation: ToolActivationRecord,
    worker: &str,
) -> VmResult<ToolActivationRecord> {
    let global = GlobalConfig::load()?;
    let mut targets = if global.tools.contains_key(&activation.tool) {
        vm_ops::resolve_fleet_targets(
            &FleetArgs {
                fleet: true,
                provider: None,
                pattern: None,
            },
            InstanceStateFilter::Any,
        )?
    } else {
        Vec::new()
    };
    targets
        .sort_by(|left, right| (&left.provider, &left.name).cmp(&(&right.provider, &right.name)));
    let targets = targets
        .into_iter()
        .map(|instance| ToolActivationTargetPlan {
            target_id: target_id(&instance.provider, &instance.name),
            environment: instance.name,
            provider: instance.provider,
            initially_running: vm_ops::is_running_status(&instance.status),
        })
        .collect();
    client
        .plan_tool_activation(
            &activation.activation_id,
            &PlanToolActivationRequest {
                worker: worker.to_string(),
                targets,
                idempotency_key: format!("plan-{}", activation.activation_id),
            },
        )
        .await
        .map_err(VmError::from)
}

async fn activate_target(
    client: &vm_packages::PackageInfrastructureClient,
    activation: &ToolActivationRecord,
    target_id: &str,
    worker: &str,
) -> VmResult<()> {
    let target = activation
        .targets
        .iter()
        .find(|target| target.target_id == target_id)
        .ok_or_else(|| VmError::validation("Tool activation target is missing", None::<String>))?;
    let deadline = tokio::time::Instant::now() + TARGET_RETRY_TIMEOUT;
    let mut last_error = None;
    while tokio::time::Instant::now() < deadline {
        match activate_environment(&activation.tool, &target.provider, &target.environment).await {
            Ok(()) => {
                client
                    .update_tool_activation_target(
                        &activation.activation_id,
                        target_id,
                        &UpdateToolActivationTargetRequest {
                            worker: worker.to_string(),
                            state: ToolActivationTargetState::Active,
                            error: None,
                            idempotency_key: target_update_key(activation, target_id, "active"),
                        },
                    )
                    .await?;
                return Ok(());
            }
            Err(error) => last_error = Some(error),
        }
        tokio::time::sleep(TARGET_RETRY_INTERVAL).await;
    }
    let error = last_error
        .map(|error| error.to_string())
        .unwrap_or_else(|| "tool activation timed out".into());
    client
        .update_tool_activation_target(
            &activation.activation_id,
            target_id,
            &UpdateToolActivationTargetRequest {
                worker: worker.to_string(),
                state: ToolActivationTargetState::Failed,
                error: Some(error),
                idempotency_key: target_update_key(activation, target_id, "failed"),
            },
        )
        .await?;
    Ok(())
}

async fn activate_environment(tool: &str, provider: &str, environment: &str) -> VmResult<()> {
    let instance = InstanceInfo {
        name: environment.to_string(),
        id: String::new(),
        status: "planned".into(),
        provider: provider.to_string(),
        project: None,
        uptime: None,
        created_at: None,
    };
    let mut subject = load_runtime_subject_for_instance(None, None, &instance)?;
    updates::activate_tool(&mut subject, tool).await
}

async fn finish(
    client: &vm_packages::PackageInfrastructureClient,
    activation: &ToolActivationRecord,
    worker: &str,
) -> VmResult<ToolActivationRecord> {
    let current = client
        .tool_activation_for_release(&activation.release_id)
        .await?;
    let state = current
        .targets
        .iter()
        .map(|target| format!("{}:{:?}", target.target_id, target.state))
        .collect::<Vec<_>>()
        .join("\0");
    let revision = &vm_packages::sha256_hex(state)[..16];
    client
        .finish_tool_activation(
            &activation.activation_id,
            &FinishToolActivationRequest {
                worker: worker.to_string(),
                idempotency_key: format!("finish-{}-{revision}", activation.activation_id),
            },
        )
        .await
        .map_err(VmError::from)
}

fn target_update_key(activation: &ToolActivationRecord, target_id: &str, outcome: &str) -> String {
    let attempt = activation
        .targets
        .iter()
        .find(|target| target.target_id == target_id)
        .map_or(1, |target| target.attempts.saturating_add(1));
    format!(
        "{outcome}-{}-{target_id}-{attempt}",
        activation.activation_id
    )
}

async fn reconcile_running_environments() -> VmResult<()> {
    let instances = vm_ops::resolve_fleet_targets(
        &FleetArgs {
            fleet: true,
            provider: None,
            pattern: None,
        },
        InstanceStateFilter::Running,
    )?;
    for instance in instances {
        match load_runtime_subject_for_instance(None, None, &instance) {
            Ok(subject) => reconcile_subject(&subject).await?,
            Err(error) => vm_warning!("{}: {}", instance.name, error),
        }
    }
    Ok(())
}

fn target_id(provider: &str, environment: &str) -> String {
    let digest = vm_packages::sha256_hex(format!("{provider}\0{environment}"));
    format!("target-{}", &digest[..32])
}

fn worker_id() -> VmResult<String> {
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
        Ok(Self {
            lock: root.join("activation-worker.lock"),
            pid: root.join("activation-worker.pid"),
            log: root.join("activation-worker.log"),
            root,
        })
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
    fn target_and_worker_ids_are_stable_managed_components() {
        let target = target_id("docker", "typemill-dev");
        assert_eq!(target, target_id("docker", "typemill-dev"));
        assert!(vm_packages::validate_managed_id("target", &target).is_ok());
        assert!(vm_packages::validate_managed_id("worker", &worker_id().unwrap()).is_ok());
    }

    #[test]
    fn launchd_worker_can_resolve_host_providers() {
        let service = launchd_service("/Users/example/.local/bin/vm");

        assert!(service.contains("<key>EnvironmentVariables</key>"));
        assert!(service.contains("/opt/homebrew/bin"));
        assert!(service.contains("/usr/local/bin"));
    }
}
