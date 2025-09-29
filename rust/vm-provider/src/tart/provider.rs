use crate::{
    common::instance::{InstanceInfo, InstanceResolver},
    progress::ProgressReporter,
    Provider, VmError,
};
use std::path::Path;
use vm_config::config::VmConfig;
use vm_core::command_stream::{is_tool_installed, stream_command};
use vm_core::error::Result;
use vm_core::{
    messages::{messages::MESSAGES, msg},
    vm_error, vm_println,
};

use super::instance::TartInstanceManager;

pub struct TartProvider {
    config: VmConfig,
}

impl TartProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("tart") {
            return Err(VmError::Dependency("Tart".into()).into());
        }
        Ok(Self { config })
    }

    fn vm_name(&self) -> String {
        self.config
            .project
            .as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project")
            .to_string()
    }

    /// Create instance manager for multi-instance operations
    fn instance_manager(&self) -> TartInstanceManager<'_> {
        TartInstanceManager::new(&self.config)
    }

    /// Resolve VM name with instance support
    fn vm_name_with_instance(&self, instance: Option<&str>) -> Result<String> {
        match instance {
            Some(_) => {
                let manager = self.instance_manager();
                manager.resolve_instance_name(instance)
            }
            None => Ok(self.vm_name()), // Use existing default behavior for backward compatibility
        }
    }
}

impl Provider for TartProvider {
    fn name(&self) -> &'static str {
        "tart"
    }

    fn create(&self) -> Result<()> {
        let progress = ProgressReporter::new();
        let main_phase = progress.start_phase("Creating Tart VM");

        // Check if VM already exists
        ProgressReporter::task(&main_phase, "Checking if VM exists...");
        let list_output = std::process::Command::new("tart").args(["list"]).output();

        if let Ok(output) = list_output {
            let list_str = String::from_utf8_lossy(&output.stdout);
            if list_str.contains(&self.vm_name()) {
                ProgressReporter::task(&main_phase, "VM already exists.");
                vm_println!(
                    "{}",
                    msg!(MESSAGES.provider_tart_vm_exists, name = self.vm_name())
                );
                vm_println!("{}", MESSAGES.provider_tart_recreate_hint);
                ProgressReporter::finish_phase(&main_phase, "Skipped creation.");
                return Ok(());
            }
        }
        ProgressReporter::task(&main_phase, "VM not found, proceeding with creation.");

        // Get image from config
        let image = self
            .config
            .tart
            .as_ref()
            .and_then(|t| t.image.as_deref())
            .unwrap_or("ghcr.io/cirruslabs/ubuntu:latest");

        // Clone the base image
        ProgressReporter::task(&main_phase, &format!("Cloning image '{}'...", image));
        let clone_result = stream_command("tart", &["clone", image, &self.vm_name()]);
        if clone_result.is_err() {
            ProgressReporter::task(&main_phase, "Clone failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return clone_result;
        }
        ProgressReporter::task(&main_phase, "Image cloned successfully.");

        // Configure VM with memory/CPU settings if specified
        if let Some(vm_config) = &self.config.vm {
            if let Some(memory) = &vm_config.memory {
                match memory.to_mb() {
                    Some(mb) => {
                        ProgressReporter::task(
                            &main_phase,
                            &format!("Setting memory to {} MB...", mb),
                        );
                        stream_command(
                            "tart",
                            &["set", &self.vm_name(), "--memory", &mb.to_string()],
                        )?;
                        ProgressReporter::task(&main_phase, "Memory configured.");
                    }
                    None => {
                        ProgressReporter::task(
                            &main_phase,
                            "Memory set to unlimited (no Tart limit).",
                        );
                    }
                }
            }

            if let Some(cpus) = vm_config.cpus {
                ProgressReporter::task(&main_phase, &format!("Setting CPUs to {}...", cpus));
                stream_command(
                    "tart",
                    &["set", &self.vm_name(), "--cpu", &cpus.to_string()],
                )?;
                ProgressReporter::task(&main_phase, "CPUs configured.");
            }
        }

        // Set disk size if specified
        if let Some(tart_config) = &self.config.tart {
            if let Some(disk_size) = tart_config.disk_size {
                ProgressReporter::task(
                    &main_phase,
                    &format!("Setting disk size to {} GB...", disk_size),
                );
                stream_command(
                    "tart",
                    &[
                        "set",
                        &self.vm_name(),
                        "--disk-size",
                        &disk_size.to_string(),
                    ],
                )?;
                ProgressReporter::task(&main_phase, "Disk size configured.");
            }
        }

        // Start VM
        ProgressReporter::task(&main_phase, "Starting VM...");
        let start_result = stream_command("tart", &["run", "--no-graphics", &self.vm_name()]);
        if start_result.is_err() {
            ProgressReporter::task(&main_phase, "VM start failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return start_result;
        }

        ProgressReporter::task(&main_phase, "VM started successfully.");
        ProgressReporter::finish_phase(&main_phase, "Environment ready.");

        vm_println!("{}", MESSAGES.provider_tart_created_success);
        vm_println!("{}", MESSAGES.provider_tart_connect_hint);
        Ok(())
    }

    fn create_instance(&self, instance_name: &str) -> Result<()> {
        let progress = ProgressReporter::new();
        let main_phase =
            progress.start_phase(&format!("Creating Tart VM instance '{}'", instance_name));

        // Get VM name with instance suffix
        let vm_name_with_suffix = format!("{}-{}", self.vm_name(), instance_name);

        // Check if VM already exists
        ProgressReporter::task(&main_phase, "Checking if VM instance exists...");
        let list_output = std::process::Command::new("tart").args(["list"]).output();

        if let Ok(output) = list_output {
            let list_str = String::from_utf8_lossy(&output.stdout);
            if list_str.contains(&vm_name_with_suffix) {
                ProgressReporter::task(&main_phase, "VM instance already exists.");
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.provider_tart_vm_exists,
                        name = &vm_name_with_suffix
                    )
                );
                vm_println!(
                    "{}",
                    msg!(
                        MESSAGES.provider_tart_vm_recreate_hint,
                        name = &vm_name_with_suffix
                    )
                );
                ProgressReporter::finish_phase(&main_phase, "Skipped creation.");
                return Ok(());
            }
        }
        ProgressReporter::task(
            &main_phase,
            "VM instance not found, proceeding with creation.",
        );

        // Get image from config
        let image = self
            .config
            .tart
            .as_ref()
            .and_then(|t| t.image.as_deref())
            .unwrap_or("ghcr.io/cirruslabs/ubuntu:latest");

        // Clone the base image
        ProgressReporter::task(
            &main_phase,
            &format!(
                "Cloning image '{}' for instance '{}'...",
                image, instance_name
            ),
        );
        let clone_result = stream_command("tart", &["clone", image, &vm_name_with_suffix]);
        if clone_result.is_err() {
            ProgressReporter::task(&main_phase, "Clone failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return clone_result;
        }
        ProgressReporter::task(&main_phase, "Image cloned successfully.");

        // Configure VM with memory/CPU settings if specified
        if let Some(vm_config) = &self.config.vm {
            if let Some(memory) = &vm_config.memory {
                match memory.to_mb() {
                    Some(mb) => {
                        ProgressReporter::task(
                            &main_phase,
                            &format!("Setting memory to {} MB...", mb),
                        );
                        stream_command(
                            "tart",
                            &["set", &vm_name_with_suffix, "--memory", &mb.to_string()],
                        )?;
                        ProgressReporter::task(&main_phase, "Memory configured.");
                    }
                    None => {
                        ProgressReporter::task(&main_phase, "Memory set to unlimited");
                    }
                }
            }

            if let Some(cpus) = vm_config.cpus {
                ProgressReporter::task(&main_phase, &format!("Setting CPUs to {}...", cpus));
                stream_command(
                    "tart",
                    &["set", &vm_name_with_suffix, "--cpu", &cpus.to_string()],
                )?;
                ProgressReporter::task(&main_phase, "CPU count configured.");
            }

            // Note: disk_size is not available in VmSettings, skipping disk configuration
        }

        // Start VM
        ProgressReporter::task(&main_phase, "Starting VM instance...");
        let start_result = stream_command("tart", &["run", "--no-graphics", &vm_name_with_suffix]);
        if start_result.is_err() {
            ProgressReporter::task(&main_phase, "VM instance start failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return start_result;
        }

        ProgressReporter::task(&main_phase, "VM instance started successfully.");
        ProgressReporter::finish_phase(&main_phase, "Environment ready.");

        vm_println!(
            "{}",
            msg!(
                MESSAGES.provider_tart_vm_created,
                name = instance_name,
                image = image
            )
        );
        vm_println!(
            "{}",
            msg!(
                MESSAGES.provider_tart_vm_connect_hint,
                name = vm_name_with_suffix
            )
        );
        Ok(())
    }

    fn start(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        stream_command("tart", &["run", "--no-graphics", &vm_name])
    }

    fn stop(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        stream_command("tart", &["stop", &vm_name])
    }

    fn destroy(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        stream_command("tart", &["delete", &vm_name])
    }

    fn ssh(&self, container: Option<&str>, _relative_path: &Path) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        duct::cmd("tart", &["ssh", &vm_name]).run()?;
        Ok(())
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        let mut args = vec!["ssh", &vm_name, "--"];
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("tart", &args)
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        // Try to read logs from ~/.tart/vms/{name}/app.log
        let home_env = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let log_path = format!("{}/.tart/vms/{}/app.log", home_env, vm_name);

        // Check if log file exists before attempting to tail
        if !Path::new(&log_path).exists() {
            let error_msg = format!("Log file not found at: {}", log_path);
            vm_error!("{}", error_msg);
            vm_println!("{}", MESSAGES.provider_logs_unavailable);
            vm_println!(
                "{}",
                msg!(MESSAGES.provider_logs_expected_location, name = vm_name)
            );
            return Err(VmError::Internal(error_msg));
        }

        vm_println!("{}", msg!(MESSAGES.provider_logs_showing, path = &log_path));
        vm_println!("{}", MESSAGES.press_ctrl_c_to_stop);

        // Use tail -f to follow the log file
        stream_command("tail", &["-f", &log_path])
    }

    fn status(&self, container: Option<&str>) -> Result<()> {
        match container {
            Some(_) => {
                // Show specific VM status
                let vm_name = self.vm_name_with_instance(container)?;
                let output = std::process::Command::new("tart").args(["list"]).output()?;

                if !output.status.success() {
                    return Ok(());
                }

                let list_output = String::from_utf8_lossy(&output.stdout);
                for line in list_output.lines() {
                    if line.contains(&vm_name) {
                        vm_println!("{}", line);
                        return Ok(());
                    }
                }
                vm_println!("{}", msg!(MESSAGES.provider_vm_not_found, name = vm_name));
                Ok(())
            }
            None => {
                // Show all VMs (existing behavior)
                stream_command("tart", &["list"])
            }
        }
    }

    fn restart(&self, container: Option<&str>) -> Result<()> {
        // Stop then start the VM
        self.stop(container)?;
        self.start(container)
    }

    fn provision(&self, _container: Option<&str>) -> Result<()> {
        // Provisioning not supported for Tart
        vm_println!("{}", MESSAGES.provider_provisioning_unsupported);
        vm_println!("{}", MESSAGES.provider_provisioning_explanation);
        Ok(())
    }

    fn list(&self) -> Result<()> {
        // List all Tart VMs
        stream_command("tart", &["list"])
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        // Force stop the VM
        stream_command("tart", &["stop", &vm_name])
    }

    fn get_sync_directory(&self) -> String {
        // Return workspace_path from config
        self.config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace")
            .to_string()
    }

    fn supports_multi_instance(&self) -> bool {
        true
    }

    fn resolve_instance_name(&self, instance: Option<&str>) -> Result<String> {
        let manager = self.instance_manager();
        manager.resolve_instance_name(instance)
    }

    fn list_instances(&self) -> Result<Vec<InstanceInfo>> {
        let manager = self.instance_manager();
        manager.list_instances()
    }
}
