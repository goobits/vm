//! Container provisioning orchestration
use super::LifecycleOperations;
use crate::container::UserConfig;
use crate::context::ProviderContext;
use fs2::FileExt;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use vm_core::command_stream::stream_command_with_timeout;
use vm_core::error::{Result, VmError};

use super::{
    ANSIBLE_PLAYBOOK_PATH, CONTAINER_READINESS_MAX_ATTEMPTS, CONTAINER_READINESS_SLEEP_SECONDS,
};

// Default timeout for Ansible provisioning (5 minutes)
// Can be overridden via environment variable ANSIBLE_TIMEOUT
const DEFAULT_ANSIBLE_TIMEOUT_SECS: u64 = 300;

static REPAIRED_HOMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

impl<'a> LifecycleOperations<'a> {
    /// Apply the shared, versioned guest-home repair before provisioning or use.
    pub(super) fn repair_home_state(
        executable: &str,
        container_name: &str,
        user_config: &UserConfig,
    ) -> Result<()> {
        use std::process::Command;

        let identity = Command::new(executable)
            .args([
                "inspect",
                "--type",
                "container",
                "--format",
                "{{.Id}}",
                container_name,
            ])
            .output()
            .map_err(|error| {
                VmError::Internal(format!("Failed to inspect guest home state: {error}"))
            })?;
        if !identity.status.success() {
            return Err(VmError::Internal(format!(
                "Failed to inspect guest home state for {container_name}"
            )));
        }
        let identity = String::from_utf8_lossy(&identity.stdout).trim().to_string();
        if identity.is_empty() {
            return Err(VmError::Internal(format!(
                "Container identity is unavailable for {container_name}"
            )));
        }
        let receipt_root = vm_core::user_paths::vm_state_dir()?.join("home-repair");
        Self::repair_home_state_for_identity(
            executable,
            container_name,
            user_config,
            &identity,
            &receipt_root,
        )
    }

    pub(super) fn repair_home_state_for_identity(
        executable: &str,
        container_name: &str,
        user_config: &UserConfig,
        identity: &str,
        receipt_root: &Path,
    ) -> Result<()> {
        use std::process::Command;

        let home_dir = format!("/home/{}", user_config.username);
        let repair_key = format!("{executable}\0{identity}\0{home_dir}");
        let mut repaired = REPAIRED_HOMES
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .map_err(|_| VmError::Internal("Guest home-state repair lock is poisoned".into()))?;
        if repaired.contains(&repair_key) {
            return Ok(());
        }
        std::fs::create_dir_all(receipt_root)?;
        let receipt_key = hex_digest(format!("{container_name}\0{}", user_config.username));
        let receipt = receipt_root.join(format!("{receipt_key}.receipt"));
        let lock_path = receipt_root.join(format!("{receipt_key}.lock"));
        let expected = hex_digest(format!(
            "{identity}\0{home_dir}\0{}",
            crate::resources::HOME_STATE_REPAIR
        ));
        if receipt_matches(&receipt, &expected) {
            repaired.insert(repair_key);
            return Ok(());
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        lock.lock_exclusive()?;
        if receipt_matches(&receipt, &expected) {
            repaired.insert(repair_key);
            return Ok(());
        }
        let output = Command::new(executable)
            .args([
                "exec",
                "-u",
                "root",
                container_name,
                "bash",
                "-c",
                crate::resources::HOME_STATE_REPAIR,
                "vm-home-repair",
                &home_dir,
                &user_config.username,
            ])
            .output()
            .map_err(|e| VmError::Internal(format!("Failed to repair guest home state: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VmError::Internal(format!(
                "Guest home-state repair failed for {}: {}",
                user_config.username,
                stderr.trim()
            )));
        }

        vm_core::file_system::atomic_write(&receipt, format!("{expected}\n").as_bytes())?;
        repaired.insert(repair_key);
        Ok(())
    }

    /// Run Ansible provisioning on a container
    fn run_ansible_provisioning(
        executable: &str,
        container_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        // Allow timeout override via environment variable for debugging
        let timeout_secs = std::env::var("ANSIBLE_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_ANSIBLE_TIMEOUT_SECS);

        // Enable timing instrumentation if ANSIBLE_PROFILE is set
        let enable_profiling = std::env::var("ANSIBLE_PROFILE").is_ok();

        // Snapshots skip base system tasks but still run layered provisioning.
        let extra_vars = format!("--extra-vars 'base_preprovisioned={}'", context.is_snapshot);

        let ansible_cmd = if enable_profiling {
            format!(
                "ANSIBLE_CALLBACKS_ENABLED=profile_tasks ansible-playbook -i localhost, -c local {} {} {}",
                ANSIBLE_PLAYBOOK_PATH,
                context.ansible_verbosity(),
                extra_vars
            )
        } else {
            format!(
                "ansible-playbook -i localhost, -c local {} {} {}",
                ANSIBLE_PLAYBOOK_PATH,
                context.ansible_verbosity(),
                extra_vars
            )
        };

        // Use a throwaway HOME under /tmp to avoid touching /home/developer (which may have wrong UID from snapshots)
        // This is much faster than chown -R on the entire home directory
        let ansible_cmd_with_fix = format!(
            "mkdir -p /tmp/ansible-home /tmp/ansible-local /tmp/ansible-remote && \
             HOME=/tmp/ansible-home ANSIBLE_LOCAL_TEMP=/tmp/ansible-local ANSIBLE_REMOTE_TEMP=/tmp/ansible-remote {}",
            ansible_cmd
        );

        stream_command_with_timeout(
            executable,
            &["exec", container_name, "bash", "-c", &ansible_cmd_with_fix],
            Some(timeout_secs),
        )
        .map_err(|e| {
            VmError::Internal(format!(
                "Ansible provisioning failed. The playbook exited with an error. Re-run `vm run linux` with debug logging for full output. Error: {e}"
            ))
        })?;

        Ok(())
    }

    /// Wait for container to become ready with exponential backoff
    async fn wait_for_container_ready_async(&self, container_name: &str) -> Result<()> {
        use tokio::time::{sleep, Duration, Instant};

        let start = Instant::now();
        let max_duration = Duration::from_secs(
            u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS,
        );

        // Exponential backoff: 100ms, 200ms, 400ms, 800ms, 1s, 1s, ...
        let mut backoff = Duration::from_millis(100);
        let max_backoff = Duration::from_secs(1);

        loop {
            if crate::container::ContainerOps::test_container_readiness(
                Some(self.executable),
                container_name,
            ) {
                return Ok(());
            }

            if start.elapsed() >= max_duration {
                return Err(VmError::Internal(format!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    container_name,
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                )));
            }

            sleep(backoff).await;
            backoff = (backoff * 2).min(max_backoff);
        }
    }

    /// Wait for container to become ready (synchronous wrapper)
    fn wait_for_container_ready(&self, container_name: &str) -> Result<()> {
        // Use block_in_place to call async code from sync context within an existing runtime
        // This avoids creating a nested runtime which would panic
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(self.wait_for_container_ready_async(container_name))
        })
    }

    /// Re-provision existing container (public API)
    pub fn provision_existing(&self, container: Option<&str>) -> Result<()> {
        let context = ProviderContext::default();
        let target_container = self.resolve_target_container(container)?;
        let status_output = std::process::Command::new(self.executable)
            .args([
                "inspect",
                "--type",
                "container",
                "--format",
                "{{.State.Status}}",
                &target_container,
            ])
            .output()?;
        let status = String::from_utf8_lossy(&status_output.stdout)
            .trim()
            .to_owned();
        if status != "running" {
            return Err(VmError::Internal(format!(
                "Container {target_container} is not running. Start it first with 'vm run linux'"
            )));
        }

        self.provision_container_with_context(&context)
    }

    /// Internal provisioning with context
    pub(super) fn provision_container_with_context(&self, context: &ProviderContext) -> Result<()> {
        self.provision_container(None, context)
    }

    /// Provision container with custom instance name and context
    pub(super) fn provision_container_with_instance_and_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        self.provision_container(Some(instance_name), context)
    }

    fn provision_container(
        &self,
        instance_name: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let container_name = instance_name.map_or_else(
            || self.container_name(),
            |name| self.container_name_with_instance(name),
        );

        self.wait_for_container_ready(&container_name)?;

        let user_config = UserConfig::from_vm_config(self.config);
        Self::repair_home_state(self.executable, &container_name, &user_config)?;

        self.prepare_and_copy_config(&container_name)?;

        Self::run_ansible_provisioning(self.executable, &container_name, context)
    }
}

fn hex_digest(value: impl AsRef<[u8]>) -> String {
    use std::fmt::Write;

    Sha256::digest(value.as_ref())
        .iter()
        .fold(String::with_capacity(64), |mut encoded, byte| {
            write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
            encoded
        })
}

fn receipt_matches(path: &Path, expected: &str) -> bool {
    std::fs::read_to_string(path).is_ok_and(|value| value.trim() == expected)
}

#[cfg(test)]
mod tests {
    use super::LifecycleOperations;
    use crate::container::UserConfig;
    use crate::resources::HOME_STATE_REPAIR;

    #[test]
    fn home_state_repair_covers_interactive_tool_state() {
        assert!(HOME_STATE_REPAIR.contains("$home_dir/.shell_history"));
        assert!(HOME_STATE_REPAIR.contains("$home_dir/.claude/projects"));
        assert!(HOME_STATE_REPAIR.contains("$home_dir/.codex/sessions"));
        assert!(HOME_STATE_REPAIR.contains("home_is_writable"));
        assert!(HOME_STATE_REPAIR.contains("rm -f /etc/profile.d/vm-worktree-repair.sh"));
    }

    #[cfg(unix)]
    #[test]
    fn home_state_repair_receipt_is_reused_until_container_identity_changes() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("fake-docker");
        let log = directory.path().join("commands.log");
        fs::write(
            &executable,
            format!("#!/bin/sh\nprintf 'call\\n' >> '{}'\n", log.display()),
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let user = UserConfig {
            uid: 1000,
            gid: 1000,
            username: "developer".into(),
        };
        let executable = executable.to_str().unwrap();

        LifecycleOperations::repair_home_state_for_identity(
            executable,
            "demo-dev",
            &user,
            "container-1",
            directory.path(),
        )
        .unwrap();
        LifecycleOperations::repair_home_state_for_identity(
            executable,
            "demo-dev",
            &user,
            "container-1",
            directory.path(),
        )
        .unwrap();

        assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);

        LifecycleOperations::repair_home_state_for_identity(
            executable,
            "demo-dev",
            &user,
            "container-2",
            directory.path(),
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 2);
    }
}
