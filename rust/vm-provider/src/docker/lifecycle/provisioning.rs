//! Container provisioning orchestration
use super::LifecycleOperations;
use crate::context::ProviderContext;
use crate::progress::{AnsibleProgressParser, ProgressParser};
use vm_core::command_stream::{
    stream_command_with_progress_and_timeout, ProgressParser as CoreProgressParser,
};
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

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

        // Pass snapshot status to Ansible to skip redundant base system tasks
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

        stream_command_with_progress_and_timeout(
            executable,
            &["exec", container_name, "bash", "-c", &ansible_cmd],
            parser,
            Some(timeout_secs),
        )
        .map_err(|e| {
            VmError::Internal(format!(
                "Ansible provisioning failed. The playbook exited with an error. To see the full output, run `vm create --verbose`. Error: {e}"
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
                vm_error!("Container failed to become ready");
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
        let status_output = std::process::Command::new("docker")
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
                "Container {target_container} is not running. Start it first with 'vm start'"
            )));
        }

        self.provision_container_with_context(&context)
    }

    /// Internal provisioning with context
    pub(super) fn provision_container_with_context(&self, context: &ProviderContext) -> Result<()> {
        // Skip provisioning if requested (e.g., for snapshot builds from Dockerfiles)
        if context.skip_provisioning {
            return Ok(());
        }

        // Step 6: Wait for readiness and run ansible (configuration only)
        self.wait_for_container_ready(&self.container_name())?;

        self.prepare_and_copy_config()?;

        Self::run_ansible_provisioning(self.executable, &self.container_name(), context)
    }

    /// Provision container with custom instance name and context
    pub(super) fn provision_container_with_instance_and_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        // Skip provisioning if requested (e.g., for snapshot builds from Dockerfiles)
        if context.skip_provisioning {
            return Ok(());
        }

        let container_name = self.container_name_with_instance(instance_name);

        // Step 6: Wait for readiness and run ansible (configuration only)
        self.wait_for_container_ready(&container_name)?;

        self.prepare_and_copy_config()?;

        Self::run_ansible_provisioning(self.executable, &container_name, context)
    }
}
