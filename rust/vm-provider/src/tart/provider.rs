use super::{instance::TartInstanceManager, provisioner::TartProvisioner};
use crate::{
    common::instance::{extract_project_name, InstanceInfo, InstanceResolver},
    context::ProviderContext,
    progress::ProgressReporter,
    BoxConfig, Provider, ResourceUsage, ServiceStatus, TempProvider, TempVmState, VmError,
    VmStatusReport,
};
use duct::cmd;
use serde::Deserialize;
use std::path::Path;
use tracing::{error, info, warn};
use vm_cli::msg;
use vm_config::config::VmConfig;
use vm_core::command_stream::{is_tool_installed, stream_command};
use vm_core::error::Result;
use vm_messages::messages::MESSAGES;

// Constants for Tart provider
const DEFAULT_TART_IMAGE: &str = "ghcr.io/cirruslabs/ubuntu:latest";
const TART_VM_LOG_PATH: &str = ".tart/vms";

struct CollectedMetrics {
    resources: ResourceUsage,
    services: Vec<ServiceStatus>,
    uptime: Option<String>,
}

#[derive(Clone)]
pub struct TartProvider {
    config: VmConfig,
}

impl TartProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("tart") {
            return Err(VmError::Dependency("Tart".into()));
        }
        Ok(Self { config })
    }

    fn is_instance_running(&self, instance_name: &str) -> Result<bool> {
        let output = cmd!("tart", "list", "--format", "json").read()?;
        let vms: Vec<serde_json::Value> = serde_json::from_str(&output)?;
        Ok(vms
            .into_iter()
            .any(|vm| vm["name"] == instance_name && vm["state"] == "running"))
    }

    fn collect_metrics(&self, instance: &str) -> Result<CollectedMetrics> {
        let metrics_script = include_str!("scripts/collect_metrics.sh");
        let output = cmd!("tart", "exec", instance, "sh", "-c", metrics_script)
            .stderr_capture()
            .read()
            .map_err(|e| VmError::Provider(format!("SSH command failed: {}", e)))?;

        self.parse_metrics_json(&output)
    }

    fn parse_metrics_json(&self, raw: &str) -> Result<CollectedMetrics> {
        #[derive(Deserialize)]
        struct Payload {
            cpu_percent: Option<f64>,
            memory_used_mb: Option<u64>,
            memory_limit_mb: Option<u64>,
            disk_used_gb: Option<f64>,
            disk_total_gb: Option<f64>,
            uptime: Option<String>,
            services: Vec<ServiceEntry>,
        }

        #[derive(Deserialize)]
        struct ServiceEntry {
            name: String,
            is_running: bool,
        }

        let payload: Payload = serde_json::from_str(raw)
            .map_err(|e| VmError::Provider(format!("Failed to parse metrics JSON: {}", e)))?;

        let resources = ResourceUsage {
            cpu_percent: payload.cpu_percent,
            memory_used_mb: payload.memory_used_mb,
            memory_limit_mb: payload.memory_limit_mb,
            disk_used_gb: payload.disk_used_gb,
            disk_total_gb: payload.disk_total_gb,
        };

        let services = payload
            .services
            .into_iter()
            .map(|svc| ServiceStatus {
                name: svc.name,
                is_running: svc.is_running,
                port: None,
                host_port: None,
                metrics: None,
                error: None,
            })
            .collect();

        Ok(CollectedMetrics {
            resources,
            services,
            uptime: payload.uptime,
        })
    }

    fn apply_runtime_config(&self, instance: &str, config: &VmConfig) -> Result<()> {
        if let Some(cpus) = config.vm.as_ref().and_then(|v| v.cpus.as_ref()) {
            if let Some(count) = cpus.to_count() {
                info!("Setting CPU count to {}", count);
                cmd!("tart", "set", instance, "--cpu", count.to_string())
                    .run()
                    .map_err(|e| VmError::Provider(format!("Failed to set CPU: {}", e)))?;
            }
        }

        if let Some(memory) = config.vm.as_ref().and_then(|v| v.memory.as_ref()) {
            if let Some(memory_mb) = memory.to_mb() {
                info!("Setting memory to {}MB", memory_mb);
                cmd!(
                    "tart",
                    "set",
                    instance,
                    "--memory",
                    format!("{}", memory_mb)
                )
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to set memory: {}", e)))?;
            }
        }

        Ok(())
    }

    fn vm_name(&self) -> String {
        extract_project_name(&self.config).to_string()
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

    /// Get Tart OCI image with BoxConfig support
    fn get_tart_image(&self, config: &VmConfig) -> Result<String> {
        // Try new vm.box first
        if let Some(vm_settings) = &config.vm {
            if let Some(box_spec) = vm_settings.get_box_spec() {
                let box_config = BoxConfig::parse_for_tart(&box_spec)?;
                return match box_config {
                    BoxConfig::TartImage(image) => Ok(image),
                    BoxConfig::Snapshot(name) => Err(VmError::Config(format!(
                        "Use 'vm snapshot restore {}' for snapshots",
                        name
                    ))),
                    _ => Err(VmError::Internal("Invalid box type for Tart".into())),
                };
            }
        }

        // Fall back to deprecated tart.image
        if let Some(tart_config) = &config.tart {
            if let Some(image) = &tart_config.image {
                return Ok(image.clone());
            }
        }

        Ok(DEFAULT_TART_IMAGE.to_string())
    }

    /// Internal VM creation logic shared by create() and create_instance()
    fn create_vm_internal(
        &self,
        vm_name: &str,
        instance_label: Option<&str>,
        config: &VmConfig,
    ) -> Result<()> {
        let progress = ProgressReporter::new();
        let phase_msg = match instance_label {
            Some(label) => format!("Creating Tart VM instance '{}'", label),
            None => "Creating Tart VM".to_string(),
        };
        let main_phase = progress.start_phase(&phase_msg);

        // Check if VM already exists
        ProgressReporter::task(&main_phase, "Checking if VM exists...");
        let list_output = std::process::Command::new("tart").args(["list"]).output();

        let mut existing_vms = Vec::new();
        if let Ok(output) = &list_output {
            let list_str = String::from_utf8_lossy(&output.stdout);
            existing_vms = list_str.lines().map(|s| s.to_string()).collect();

            if list_str.contains(vm_name) {
                ProgressReporter::task(&main_phase, "VM already exists.");
                info!(
                    "{}",
                    msg!(MESSAGES.service.provider_tart_vm_exists, name = vm_name)
                );
                info!("{}", MESSAGES.service.provider_tart_recreate_hint);
                ProgressReporter::finish_phase(&main_phase, "Skipped creation.");
                return Ok(());
            }
        }
        ProgressReporter::task(&main_phase, "VM not found, proceeding with creation.");

        // Check for orphaned VMs (same project, different instance/suffix)
        let project_prefix = format!("{}-", extract_project_name(&self.config));
        let orphans: Vec<String> = existing_vms
            .into_iter()
            .filter(|name| name.starts_with(&project_prefix) && name != vm_name)
            .collect();

        if !orphans.is_empty() {
            warn!("Found potential orphaned VMs from previous runs/instances");
            eprintln!("\n⚠️  Warning: Other VMs for this project detected");
            eprintln!("   These VMs might be from other instances or previous runs:\n");
            for orphan in &orphans {
                eprintln!("   • {}", orphan);
            }
            eprintln!("\n💡 If these are leftovers, you can clean them up with:");
            for orphan in &orphans {
                eprintln!("      tart delete {}", orphan);
            }
            eprintln!();
        }

        // Get image from config using new BoxConfig system
        let image = self.get_tart_image(config)?;

        // Clone the base image
        ProgressReporter::task(&main_phase, &format!("Cloning image '{}'...", image));
        let clone_result = stream_command("tart", &["clone", &image, vm_name]);
        if clone_result.is_err() {
            ProgressReporter::task(&main_phase, "Clone failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return clone_result;
        }
        ProgressReporter::task(&main_phase, "Image cloned successfully.");

        // Configure VM with memory/CPU settings if specified
        if let Some(vm_config) = &config.vm {
            if let Some(memory) = &vm_config.memory {
                match memory.to_mb() {
                    Some(mb) => {
                        ProgressReporter::task(
                            &main_phase,
                            &format!("Setting memory to {} MB...", mb),
                        );
                        stream_command("tart", &["set", vm_name, "--memory", &mb.to_string()])?;
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

            if let Some(cpus) = &vm_config.cpus {
                match cpus.to_count() {
                    Some(count) => {
                        ProgressReporter::task(
                            &main_phase,
                            &format!("Setting CPUs to {}...", count),
                        );
                        stream_command("tart", &["set", vm_name, "--cpu", &count.to_string()])?;
                        ProgressReporter::task(&main_phase, "CPUs configured.");
                    }
                    None => {
                        ProgressReporter::task(
                            &main_phase,
                            "CPUs set to unlimited (no Tart limit).",
                        );
                    }
                }
            }
        }

        // Set disk size if specified
        if let Some(tart_config) = &config.tart {
            if let Some(disk_limit) = &tart_config.disk_size {
                if let Some(disk_gb) = disk_limit.to_gb() {
                    ProgressReporter::task(
                        &main_phase,
                        &format!("Setting disk size to {} GB...", disk_gb),
                    );
                    stream_command(
                        "tart",
                        &["set", vm_name, "--disk-size", &disk_gb.to_string()],
                    )?;
                    ProgressReporter::task(&main_phase, "Disk size configured.");
                }
            }
        }

        // Start VM
        ProgressReporter::task(&main_phase, "Starting VM...");
        let start_result = stream_command("tart", &["run", "--no-graphics", vm_name]);
        if start_result.is_err() {
            ProgressReporter::task(&main_phase, "VM start failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return start_result;
        }
        ProgressReporter::task(&main_phase, "VM started successfully.");

        // Run initial provisioning using the effective config
        ProgressReporter::task(&main_phase, "Running initial provisioning...");
        let provisioner = TartProvisioner::new(vm_name.to_string(), self.get_sync_directory());
        if let Err(e) = provisioner.provision(config) {
            warn!(
                "Initial provisioning failed: {}. The VM is created but may not be fully configured.",
                e
            );
            // This is treated as a hard failure for create, as an un-provisioned VM is not useful.
            ProgressReporter::finish_phase(&main_phase, "Provisioning failed.");
            return Err(e);
        }
        ProgressReporter::task(&main_phase, "Initial provisioning complete.");

        ProgressReporter::finish_phase(&main_phase, "Environment ready.");

        info!("{}", MESSAGES.service.provider_tart_created_success);
        info!("{}", MESSAGES.service.provider_tart_connect_hint);
        Ok(())
    }
}

impl Provider for TartProvider {
    fn name(&self) -> &'static str {
        "tart"
    }

    fn create(&self) -> Result<()> {
        self.create_vm_internal(&self.vm_name(), None, &self.config)
    }

    fn create_instance(&self, instance_name: &str) -> Result<()> {
        let vm_name = format!("{}-{}", self.vm_name(), instance_name);
        self.create_vm_internal(&vm_name, Some(instance_name), &self.config)
    }

    fn create_with_context(&self, context: &ProviderContext) -> Result<()> {
        // Apply global config defaults if present, but always use the project VmConfig
        let _ = context; // Global config is not directly applicable to VM creation
        self.create_vm_internal(&self.vm_name(), None, &self.config)
    }

    fn create_instance_with_context(
        &self,
        instance_name: &str,
        context: &ProviderContext,
    ) -> Result<()> {
        // Apply global config defaults if present, but always use the project VmConfig
        let _ = context; // Global config is not directly applicable to VM creation
        let vm_name = format!("{}-{}", self.vm_name(), instance_name);
        self.create_vm_internal(&vm_name, Some(instance_name), &self.config)
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

    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        use duct::cmd;

        let instance_name = self.resolve_instance_name(container)?;

        // Get the sync directory (project root in VM)
        let sync_dir = self.get_sync_directory();

        // Resolve full path in VM
        let target_path = if relative_path == Path::new("") || relative_path == Path::new(".") {
            sync_dir.clone()
        } else {
            format!(
                "{}/{}",
                sync_dir.trim_end_matches('/'),
                relative_path.display()
            )
        };

        info!("Opening SSH session in directory: {}", target_path);

        // Use `tart exec -i -t` for interactive shell session
        let ssh_command = format!("cd '{}' && exec $SHELL -l", target_path);

        cmd!(
            "tart",
            "exec",
            "-i",
            "-t",
            &instance_name,
            "sh",
            "-c",
            &ssh_command
        )
        .run()
        .map_err(|e| VmError::Provider(format!("Exec failed: {}", e)))?;

        Ok(())
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        let mut args = vec!["exec", &vm_name];
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("tart", &args)
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;
        // Try to read logs from ~/.tart/vms/{name}/app.log
        let home_env = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let log_path = format!("{}/{}/{}/app.log", home_env, TART_VM_LOG_PATH, vm_name);

        // Check if log file exists before attempting to tail
        if !Path::new(&log_path).exists() {
            let error_msg = format!("Log file not found at: {}", log_path);
            error!("{}", error_msg);
            info!("{}", MESSAGES.service.provider_logs_unavailable);
            info!(
                "{}",
                msg!(
                    MESSAGES.service.provider_logs_expected_location,
                    name = vm_name
                )
            );
            return Err(VmError::Internal(error_msg));
        }

        info!(
            "{}",
            msg!(MESSAGES.service.provider_logs_showing, path = &log_path)
        );
        info!("{}", MESSAGES.common.press_ctrl_c_to_stop);

        // Use tail -f to follow the log file
        stream_command("tail", &["-f", &log_path])
    }

    fn copy(&self, source: &str, destination: &str, container: Option<&str>) -> Result<()> {
        let vm_name = self.vm_name_with_instance(container)?;

        // Determine if we're copying to or from the VM
        let (local_path, remote_path, is_upload) = if source.contains(':') {
            // Downloading from VM
            let parts: Vec<&str> = source.splitn(2, ':').collect();
            if parts.len() == 2 {
                (destination, parts[1], false)
            } else {
                return Err(VmError::Provider("Invalid source format".to_string()));
            }
        } else if destination.contains(':') {
            // Uploading to VM
            let parts: Vec<&str> = destination.splitn(2, ':').collect();
            if parts.len() == 2 {
                (source, parts[1], true)
            } else {
                return Err(VmError::Provider("Invalid destination format".to_string()));
            }
        } else {
            // Neither has container prefix, assume uploading to VM
            (source, destination, true)
        };

        // Use scp-like approach via tart exec
        if is_upload {
            // Upload: local -> VM
            let copy_cmd = format!("cat > '{}'", remote_path.replace('\'', "'\"'\"'"));
            let output = cmd!(
                "sh",
                "-c",
                format!(
                    "cat '{}' | tart exec {} sh -c \"{}\"",
                    local_path.replace('\'', "'\"'\"'"),
                    vm_name,
                    copy_cmd
                )
            )
            .run();

            output.map_err(|e| VmError::Provider(format!("Failed to copy file to VM: {}", e)))?;
        } else {
            // Download: VM -> local
            let copy_cmd = format!("cat '{}'", remote_path.replace('\'', "'\"'\"'"));
            let result = cmd!("tart", "exec", &vm_name, "sh", "-c", &copy_cmd)
                .stdout_capture()
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to read file from VM: {}", e)))?;

            std::fs::write(local_path, result.stdout)
                .map_err(|e| VmError::Provider(format!("Failed to write local file: {}", e)))?;
        }

        Ok(())
    }

    fn status(&self, container: Option<&str>) -> Result<()> {
        match container {
            Some(_) => {
                // Show specific VM status
                let vm_name = self.vm_name_with_instance(container)?;
                let output = std::process::Command::new("tart").args(["list"]).output()?;

                if !output.status.success() {
                    return Err(VmError::Internal(
                        "Failed to get Tart VM status. Check that Tart is properly installed"
                            .to_string(),
                    ));
                }

                let list_output = String::from_utf8_lossy(&output.stdout);
                for line in list_output.lines() {
                    if line.contains(&vm_name) {
                        info!("{}", line);
                        return Ok(());
                    }
                }
                info!(
                    "{}",
                    msg!(MESSAGES.service.provider_vm_not_found, name = vm_name)
                );
                Ok(())
            }
            None => {
                // Show all VMs (existing behavior)
                stream_command("tart", &["list"])
            }
        }
    }

    fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
        let instance_name = self.resolve_instance_name(container)?;

        if !self.is_instance_running(&instance_name)? {
            return Ok(VmStatusReport {
                name: instance_name.clone(),
                provider: "tart".into(),
                is_running: false,
                ..Default::default()
            });
        }

        let metrics = self.collect_metrics(&instance_name)?;

        Ok(VmStatusReport {
            name: instance_name,
            provider: "tart".into(),
            container_id: None,
            is_running: true,
            uptime: metrics.uptime,
            resources: metrics.resources,
            services: metrics.services,
        })
    }

    fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        // Apply runtime configuration from project config
        if context.global_config.is_some() {
            info!("Applying config updates to Tart VM");
            self.apply_runtime_config(&instance_name, &self.config)?;
        }

        self.start(Some(&instance_name))
    }

    fn restart_with_context(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        // Apply runtime configuration from project config
        if context.global_config.is_some() {
            info!("Applying config updates to Tart VM");
            self.apply_runtime_config(&instance_name, &self.config)?;
        }

        self.restart(Some(&instance_name))
    }

    fn restart(&self, container: Option<&str>) -> Result<()> {
        // Stop then start the VM
        self.stop(container)?;
        self.start(container)
    }

    fn provision(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;

        let provisioner = TartProvisioner::new(instance_name.clone(), self.get_sync_directory());

        provisioner.provision(&self.config)?;

        info!("{}", MESSAGES.vm.apply_success);
        Ok(())
    }

    fn list(&self) -> Result<()> {
        // List all Tart VMs
        stream_command("tart", &["list"])
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_instance_name(container)?;
        warn!("Force killing Tart VM: {}", &instance_name);

        // Use the force flag directly for a kill operation.
        cmd!("tart", "stop", &instance_name, "--force")
            .run()
            .map_err(|e| VmError::Provider(format!("Failed to force stop VM: {}", e)))?;

        info!("Tart VM force-stopped successfully via CLI");
        Ok(())
    }

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
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

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

#[cfg(any(test, feature = "test-helpers"))]
impl TartProvider {
    /// Test-only helper method to execute commands in a specific path
    pub fn exec_in_path(
        &self,
        container: Option<&str>,
        path: &std::path::Path,
        cmd_parts: &[&str],
    ) -> Result<String> {
        use duct::cmd;
        let instance_name = self.resolve_instance_name(container)?;
        let command_str = cmd_parts.join(" ");
        let ssh_command = format!("cd '{}' && {}", path.display(), command_str);

        let output = cmd!("tart", "exec", &instance_name, "sh", "-c", &ssh_command)
            .read()
            .map_err(|e| VmError::Provider(format!("Exec in path failed: {}", e)))?;

        Ok(output)
    }
}

impl TempProvider for TartProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        info!("Updating mounts for Tart VM: {}", state.container_name);
        self.stop(Some(&state.container_name))?;
        self.recreate_with_mounts(state)?;
        Ok(())
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        for mount in &state.mounts {
            let mount_arg = format!("{}:{}", mount.source.display(), mount.target.display());

            info!("Adding mount: {}", mount_arg);

            cmd!("tart", "set", &state.container_name, "--dir", &mount_arg)
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to add mount: {}", e)))?;
        }

        self.start(Some(&state.container_name))?;
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        if !self.is_instance_running(container_name)? {
            return Ok(false);
        }

        let ssh_test = cmd!("tart", "exec", container_name, "echo", "healthy")
            .stderr_null()
            .stdout_null()
            .run();

        Ok(ssh_test.is_ok())
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        self.is_instance_running(container_name)
    }
}
