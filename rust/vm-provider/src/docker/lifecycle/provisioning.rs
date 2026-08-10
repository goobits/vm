//! Container provisioning orchestration
use super::LifecycleOperations;
use crate::context::ProviderContext;
use crate::docker::UserConfig;
use crate::progress::{AnsibleProgressParser, ProgressParser};
use vm_core::command_stream::{
    stream_command_with_progress_and_timeout, ProgressParser as CoreProgressParser,
};
use vm_core::error::{Result, VmError};

use super::{
    ANSIBLE_PLAYBOOK_PATH, CONTAINER_READINESS_MAX_ATTEMPTS, CONTAINER_READINESS_SLEEP_SECONDS,
};

// Default timeout for Ansible provisioning (5 minutes)
// Can be overridden via environment variable ANSIBLE_TIMEOUT
const DEFAULT_ANSIBLE_TIMEOUT_SECS: u64 = 300;

/// Adapter to convert AnsibleProgressParser to CoreProgressParser trait
struct AnsibleParserAdapter(AnsibleProgressParser);

impl CoreProgressParser for AnsibleParserAdapter {
    fn parse_line(&mut self, line: &str) {
        ProgressParser::parse_line(&mut self.0, line);
    }
    fn finish(&self) {
        ProgressParser::finish(&self.0);
    }
}

impl<'a> LifecycleOperations<'a> {
    /// Apply the shared, versioned guest-home repair before provisioning or use.
    pub(super) fn repair_home_state(
        executable: &str,
        container_name: &str,
        user_config: &UserConfig,
    ) -> Result<()> {
        use std::process::Command;

        let home_dir = format!("/home/{}", user_config.username);
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

        Ok(())
    }

    /// Run Ansible provisioning on a container
    fn run_ansible_provisioning(
        executable: &str,
        container_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        let parser = if context.is_verbose() {
            None
        } else {
            Some(
                Box::new(AnsibleParserAdapter(AnsibleProgressParser::new(false)))
                    as Box<dyn CoreProgressParser>,
            )
        };

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

        stream_command_with_progress_and_timeout(
            executable,
            &["exec", container_name, "bash", "-c", &ansible_cmd_with_fix],
            parser,
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
            if crate::docker::DockerOps::test_container_readiness(
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

#[cfg(test)]
mod tests {
    use crate::resources::HOME_STATE_REPAIR;

    #[test]
    fn home_state_repair_covers_interactive_tool_state() {
        assert!(HOME_STATE_REPAIR.contains("$home_dir/.shell_history"));
        assert!(HOME_STATE_REPAIR.contains("$home_dir/.claude/projects"));
        assert!(HOME_STATE_REPAIR.contains("$home_dir/.codex/sessions"));
        assert!(HOME_STATE_REPAIR.contains("home_is_writable"));
    }
}
