use crate::{error::ProviderError, progress::ProgressReporter, Provider};
use anyhow::Result;
use std::path::Path;
use vm_common::command_stream::{is_tool_installed, stream_command};
use vm_common::{vm_error, vm_println};
use vm_config::config::VmConfig;

pub struct TartProvider {
    config: VmConfig,
}

impl TartProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("tart") {
            return Err(ProviderError::DependencyNotFound("Tart".into()).into());
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
                println!("⚠️  Tart VM '{}' already exists.", self.vm_name());
                println!("To recreate, first run: vm destroy");
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

        println!("\n✅ Tart VM created successfully!");
        println!("💡 Use 'vm ssh' to connect to the VM");
        Ok(())
    }

    fn start(&self, _container: Option<&str>) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        stream_command("tart", &["run", "--no-graphics", &self.vm_name()])
    }

    fn stop(&self, _container: Option<&str>) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        stream_command("tart", &["stop", &self.vm_name()])
    }

    fn destroy(&self, _container: Option<&str>) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        stream_command("tart", &["delete", &self.vm_name()])
    }

    fn ssh(&self, _container: Option<&str>, _relative_path: &Path) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        duct::cmd("tart", &["ssh", &self.vm_name()]).run()?;
        Ok(())
    }

    fn exec(&self, _container: Option<&str>, cmd: &[String]) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        let vm = self.vm_name();
        let mut args = vec!["ssh", &vm, "--"];
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("tart", &args)
    }

    fn logs(&self, _container: Option<&str>) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        // Try to read logs from ~/.tart/vms/{name}/app.log
        let home_env = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let log_path = format!("{}/.tart/vms/{}/app.log", home_env, self.vm_name());

        // Check if log file exists before attempting to tail
        if !Path::new(&log_path).exists() {
            let error_msg = format!("Log file not found at: {}", log_path);
            vm_error!("{}", error_msg);
            vm_println!("The VM might not be running or logs may not be available yet.");
            vm_println!("Expected location: ~/.tart/vms/{}/app.log", self.vm_name());
            return Err(anyhow::anyhow!(error_msg));
        }

        vm_println!("Showing Tart VM logs from: {}", log_path);
        println!("Press Ctrl+C to stop...\n");

        // Use tail -f to follow the log file
        stream_command("tail", &["-f", &log_path])
    }

    fn status(&self, _container: Option<&str>) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        stream_command("tart", &["list"])
    }

    fn restart(&self, _container: Option<&str>) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        // Stop then start the VM
        self.stop(None)?;
        self.start(None)
    }

    fn provision(&self, _container: Option<&str>) -> Result<()> {
        // Note: Tart doesn't support multiple containers, container parameter is ignored
        // Provisioning not supported for Tart
        println!("Provisioning not supported for Tart VMs");
        println!("Tart VMs use pre-built images and don't support dynamic provisioning");
        Ok(())
    }

    fn list(&self) -> Result<()> {
        // List all Tart VMs
        stream_command("tart", &["list"])
    }

    fn kill(&self, _container: Option<&str>) -> Result<()> {
        // Force stop the VM
        stream_command("tart", &["stop", &self.vm_name()])
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
}
