//! Container provisioning orchestration
use super::LifecycleOperations;
use crate::context::ProviderContext;
use crate::progress::{AnsibleProgressParser, ProgressParser};
use vm_core::command_stream::{stream_command_with_progress, ProgressParser as CoreProgressParser};
use vm_core::error::{Result, VmError};
use vm_core::vm_error;

use super::{
    ANSIBLE_PLAYBOOK_PATH, CONTAINER_READINESS_MAX_ATTEMPTS, CONTAINER_READINESS_SLEEP_SECONDS,
};

impl<'a> LifecycleOperations<'a> {
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
        // Step 6: Wait for readiness and run ansible (configuration only)
        let mut attempt = 1;
        while attempt <= CONTAINER_READINESS_MAX_ATTEMPTS {
            if crate::docker::DockerOps::test_container_readiness(&self.container_name()) {
                break;
            }
            if attempt == CONTAINER_READINESS_MAX_ATTEMPTS {
                vm_error!("Container failed to become ready");
                return Err(VmError::Internal(format!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    self.container_name(),
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                )));
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
            attempt += 1;
        }

        self.prepare_and_copy_config()?;

        // Use progress-aware streaming with AnsibleProgressParser
        let parser = if context.is_verbose() {
            None
        } else {
            // Implement the trait adapter
            struct AnsibleParserAdapter(AnsibleProgressParser);
            impl CoreProgressParser for AnsibleParserAdapter {
                fn parse_line(&mut self, line: &str) {
                    ProgressParser::parse_line(&mut self.0, line);
                }
                fn finish(&self) {
                    ProgressParser::finish(&self.0);
                }
            }
            Some(
                Box::new(AnsibleParserAdapter(AnsibleProgressParser::new(false)))
                    as Box<dyn CoreProgressParser>,
            )
        };

        stream_command_with_progress(
            "docker",
            &[
                "exec",
                &self.container_name(),
                "bash",
                "-c",
                &format!(
                    "ansible-playbook -i localhost, -c local {} {}",
                    ANSIBLE_PLAYBOOK_PATH,
                    context.ansible_verbosity()
                ),
            ],
            parser,
        )
        .map_err(|e| {
            VmError::Internal(format!(
                "Ansible provisioning failed. The playbook exited with an error. To see the full output, run `vm create --verbose`. Error: {e}"
            ))
        })?;

        Ok(())
    }

    /// Provision container with custom instance name and context
    pub(super) fn provision_container_with_instance_and_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        let container_name = self.container_name_with_instance(instance_name);

        // Step 6: Wait for readiness and run ansible (configuration only)
        let mut attempt = 1;
        while attempt <= CONTAINER_READINESS_MAX_ATTEMPTS {
            if crate::docker::DockerOps::test_container_readiness(&container_name) {
                break;
            }
            if attempt == CONTAINER_READINESS_MAX_ATTEMPTS {
                vm_error!("Container failed to become ready");
                return Err(VmError::Internal(format!(
                    "Container '{}' failed to become ready after {} seconds. Container may be unhealthy or not starting properly",
                    container_name,
                    u64::from(CONTAINER_READINESS_MAX_ATTEMPTS) * CONTAINER_READINESS_SLEEP_SECONDS
                )));
            }
            std::thread::sleep(std::time::Duration::from_secs(
                CONTAINER_READINESS_SLEEP_SECONDS,
            ));
            attempt += 1;
        }

        self.prepare_and_copy_config()?;

        // Use progress-aware streaming with AnsibleProgressParser
        let parser = if context.is_verbose() {
            None
        } else {
            // Implement the trait adapter
            struct AnsibleParserAdapter(AnsibleProgressParser);
            impl CoreProgressParser for AnsibleParserAdapter {
                fn parse_line(&mut self, line: &str) {
                    ProgressParser::parse_line(&mut self.0, line);
                }
                fn finish(&self) {
                    ProgressParser::finish(&self.0);
                }
            }
            Some(
                Box::new(AnsibleParserAdapter(AnsibleProgressParser::new(false)))
                    as Box<dyn CoreProgressParser>,
            )
        };

        stream_command_with_progress(
            "docker",
            &[
                "exec",
                &container_name,
                "bash",
                "-c",
                &format!(
                    "ansible-playbook -i localhost, -c local {} {}",
                    ANSIBLE_PLAYBOOK_PATH,
                    context.ansible_verbosity()
                ),
            ],
            parser,
        )
        .map_err(|e| {
            VmError::Internal(format!(
                "Ansible provisioning failed. The playbook exited with an error. To see the full output, run `vm create --verbose`. Error: {e}"
            ))
        })?;
        Ok(())
    }
}
