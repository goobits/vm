use crate::{
    common::instance::{extract_project_name, InstanceInfo, InstanceResolver},
    context::ProviderContext,
    progress::ProgressReporter,
    security::SecurityValidator,
    Mount, MountPermission, Provider, ResourceUsage, ServiceStatus, TempProvider, TempVmState,
    VmError, VmStatusReport,
};
use std::env;
use std::path::Path;
use tracing::{info, warn};
use vm_config::{
    config::{MemoryLimit, VmConfig},
    GlobalConfig,
};
use vm_core::command_stream::{is_tool_installed, stream_command};
use vm_core::error::Result;
use vm_core::{vm_println, vm_success, vm_warning};

use super::instance::VagrantInstanceManager;

// Constants for Vagrant provider
const DEFAULT_MACHINE_NAME: &str = "default";
const DEFAULT_WORKSPACE_PATH: &str = "/workspace";
const ENV_PREFIX_VM: &str = "VM_";

/// Safely escape a string for shell execution by wrapping in single quotes
/// and escaping any existing single quotes
fn shell_escape(arg: &str) -> String {
    // If the argument contains no special characters, return as-is
    if arg
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
    {
        arg.to_string()
    } else {
        // Wrap in single quotes and escape any existing single quotes
        format!("'{}'", arg.replace('\'', "'\"'\"'"))
    }
}

#[derive(Clone)]
pub struct VagrantProvider {
    config: VmConfig,
    project_dir: std::path::PathBuf,
}

impl VagrantProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("vagrant") {
            return Err(VmError::Dependency("Vagrant".into()));
        }
        let project_dir = env::current_dir()?;
        Ok(Self {
            config,
            project_dir,
        })
    }

    fn run_vagrant_command(&self, args: &[&str]) -> Result<()> {
        // Set VAGRANT_CWD to providers/vagrant directory
        let vagrant_cwd = self.project_dir.join("providers/vagrant");

        // Create sanitized config without sensitive environment variables
        let sanitized_config = self.create_sanitized_config();
        let config_json = serde_json::to_string_pretty(&sanitized_config)?;

        // Thread-safe: Set environment variables per-command, not globally
        // Use duct to stream output with custom environment
        use std::io::{BufRead, BufReader};
        let reader = duct::cmd("vagrant", args)
            .env("VAGRANT_CWD", vagrant_cwd)
            .env("VM_PROJECT_DIR", &self.project_dir)
            .env("VM_CONFIG_JSON", config_json)
            .stderr_to_stdout()
            .reader()?;

        let lines = BufReader::new(reader).lines();
        for line in lines {
            info!("{}", line?);
        }
        Ok(())
    }

    /// Create a sanitized version of the config that excludes sensitive environment variables
    fn create_sanitized_config(&self) -> VmConfig {
        let mut sanitized = self.config.clone();

        // Remove potentially sensitive environment variables to prevent exposure
        // Only expose essential non-sensitive environment for Vagrant provisioning
        if !sanitized.environment.is_empty() {
            let safe_env_prefixes = [
                ENV_PREFIX_VM,
                "VAGRANT_",
                "LANG",
                "LC_",
                "PATH",
                "HOME",
                "USER",
            ];

            sanitized.environment.retain(|key, _| {
                // Keep environment variables that are clearly safe
                safe_env_prefixes.iter().any(|prefix| key.starts_with(prefix)) ||
                // Keep standard non-sensitive variables
                matches!(key.as_str(), "TERM" | "SHELL" | "EDITOR" | "PAGER")
            });
        }

        sanitized
    }

    /// Create instance manager for multi-machine operations
    fn instance_manager(&self) -> VagrantInstanceManager<'_> {
        VagrantInstanceManager::new(&self.config, &self.project_dir)
    }

    /// Resolve machine name with instance support
    fn resolve_machine_name(&self, instance: Option<&str>) -> Result<String> {
        match instance {
            Some(_) => {
                let manager = self.instance_manager();
                manager.resolve_instance_name(instance)
            }
            None => Ok(DEFAULT_MACHINE_NAME.to_string()), // Vagrant's default machine name
        }
    }

    /// Run vagrant command with optional machine name
    fn run_vagrant_command_with_machine(&self, args: &[&str], machine: Option<&str>) -> Result<()> {
        match machine {
            Some(machine_name) => {
                let resolved_name = self.resolve_machine_name(Some(machine_name))?;
                let mut command_args = Vec::from(args);
                command_args.push(&resolved_name);
                self.run_vagrant_command(&command_args)
            }
            None => self.run_vagrant_command(args),
        }
    }

    /// Get the directory for a specific Vagrant instance
    fn get_instance_dir(&self, instance: &str) -> Result<std::path::PathBuf> {
        let instance_manager = self.instance_manager();
        let instances = instance_manager.list_instances()?;
        if instances.iter().any(|i| i.name == instance) {
            // For now, assume a simple directory structure. This could be made more robust.
            Ok(self.project_dir.join("providers/vagrant"))
        } else {
            Err(VmError::Provider(format!(
                "Instance '{}' not found.",
                instance
            )))
        }
    }

    /// Generate Vagrantfile content based on global config
    fn generate_vagrantfile_content(&self, global_config: &GlobalConfig) -> Result<String> {
        // 1. Merge project config with global defaults to get final VM settings
        let mut vm_settings = self.config.vm.clone().unwrap_or_default();
        if vm_settings.memory.is_none() {
            if let Some(mem) = global_config.defaults.memory {
                vm_settings.memory = Some(MemoryLimit::Limited(mem));
            }
        }
        if vm_settings.cpus.is_none() {
            vm_settings.cpus = global_config.defaults.cpus;
        }

        // 2. Get all machine names for this project to define them in the Vagrantfile
        let machines = self
            .instance_manager()
            .list_instances()?
            .into_iter()
            .map(|i| i.name)
            .collect::<Vec<String>>();

        // If there are no machines, we can't generate a valid Vagrantfile
        if machines.is_empty() {
            return Err(VmError::Provider(
                "No instances found to generate Vagrantfile for.".to_string(),
            ));
        }

        // 3. Generate Vagrantfile content
        let mut content = String::new();
        content.push_str("# -*- mode: ruby -*-\n");
        content.push_str("# vi: set ft=ruby :\n\n");
        content.push_str("Vagrant.configure(\"2\") do |config|\n");

        // Add VM provider configuration from merged settings
        if let Some(memory_limit) = &vm_settings.memory {
            if let Some(mb) = memory_limit.to_mb() {
                content.push_str("  config.vm.provider \"virtualbox\" do |vb|\n");
                content.push_str(&format!("    vb.memory = \"{}\"\n", mb));
                if let Some(cpus) = vm_settings.cpus {
                    content.push_str(&format!("    vb.cpus = {}\n", cpus));
                }
                content.push_str("  end\n\n");
            }
        }

        // Add synced folder for workspace
        let workspace_path = self.get_sync_directory();
        content.push_str(&format!(
            "  config.vm.synced_folder \".\", \"{}\"\n\n",
            workspace_path
        ));

        // Get project name for hostnames
        let project_name = extract_project_name(&self.config);

        // Define each machine
        for machine_name in &machines {
            content.push_str(&format!(
                "  config.vm.define \"{}\" do |{}|\n",
                machine_name, machine_name
            ));

            let box_name = vm_settings.box_name.as_deref().unwrap_or("ubuntu/focal64");
            content.push_str(&format!("    {}.vm.box = \"{}\"\n", machine_name, box_name));

            content.push_str(&format!(
                "    {}.vm.hostname = \"{}-{}\"\n",
                machine_name, project_name, machine_name
            ));

            content.push_str("  end\n\n");
        }

        content.push_str("end\n");

        Ok(content)
    }

    /// Regenerate the Vagrantfile for an instance with updated config
    fn regenerate_vagrantfile(&self, instance: &str, global_config: &GlobalConfig) -> Result<()> {
        let instance_dir = self.get_instance_dir(instance)?;
        let vagrantfile_path = instance_dir.join("Vagrantfile");

        // Generate new Vagrantfile content
        let vagrantfile_content = self.generate_vagrantfile_content(global_config)?;

        // Write new Vagrantfile
        std::fs::write(&vagrantfile_path, vagrantfile_content)
            .map_err(|e| VmError::Provider(format!("Failed to write Vagrantfile: {}", e)))?;

        info!("Regenerated Vagrantfile at {:?}", vagrantfile_path);
        Ok(())
    }

    /// Helper: Execute SSH command and capture output
    fn ssh_exec_capture(&self, instance: &str, cmd: &str) -> Result<String> {
        let vagrant_cwd = self.project_dir.join("providers/vagrant");
        let output = duct::cmd("vagrant", &["ssh", instance, "-c", cmd])
            .dir(vagrant_cwd)
            .stderr_null()
            .read()
            .map_err(|e| VmError::Provider(format!("SSH command failed: {}", e)))?;

        Ok(output)
    }

    /// Check if a Vagrant instance is running
    fn is_instance_running(&self, instance: &str) -> Result<bool> {
        let vagrant_cwd = self.project_dir.join("providers/vagrant");
        let status_output = duct::cmd("vagrant", &["status", instance])
            .dir(vagrant_cwd)
            .read()
            .map_err(|e| VmError::Provider(format!("Failed to get Vagrant status: {}", e)))?;

        Ok(status_output.contains("running"))
    }

    /// Get resource usage (CPU, memory, disk) from the VM
    fn get_resource_usage(&self, instance: &str) -> Result<ResourceUsage> {
        // SSH into VM and run system commands
        let cpu_cmd = "top -bn1 | grep 'Cpu(s)' | awk '{print $2}' | cut -d'%' -f1";
        let mem_cmd = "free -m | awk 'NR==2{printf \"%s %s\", $3,$2}'";
        let disk_cmd = "df -BG / | awk 'NR==2{printf \"%s %s\", $3,$2}'";

        let cpu_percent = self
            .ssh_exec_capture(instance, cpu_cmd)
            .ok()
            .and_then(|s| s.trim().parse::<f64>().ok());

        let (memory_used_mb, memory_limit_mb) = self
            .ssh_exec_capture(instance, mem_cmd)
            .ok()
            .and_then(|s| {
                let parts: Vec<&str> = s.split_whitespace().collect();
                if parts.len() == 2 {
                    let used = parts[0].parse::<u64>().ok();
                    let total = parts[1].parse::<u64>().ok();
                    Some((used, total))
                } else {
                    None
                }
            })
            .unwrap_or((None, None));

        let (disk_used_gb, disk_total_gb) = self
            .ssh_exec_capture(instance, disk_cmd)
            .ok()
            .and_then(|s| {
                let parts: Vec<&str> = s.split_whitespace().collect();
                if parts.len() == 2 {
                    // Remove 'G' suffix and parse
                    let used = parts[0].trim_end_matches('G').parse::<f64>().ok();
                    let total = parts[1].trim_end_matches('G').parse::<f64>().ok();
                    Some((used, total))
                } else {
                    None
                }
            })
            .unwrap_or((None, None));

        Ok(ResourceUsage {
            cpu_percent,
            memory_used_mb,
            memory_limit_mb,
            disk_used_gb,
            disk_total_gb,
        })
    }

    /// Get the status of known services running in the VM
    fn get_service_statuses(&self, instance: &str) -> Result<Vec<ServiceStatus>> {
        let mut services = Vec::new();

        // Check for common services (PostgreSQL, Redis, MongoDB)
        for (service_name, systemd_unit) in &[
            ("PostgreSQL", "postgresql"),
            ("Redis", "redis"),
            ("MongoDB", "mongod"),
        ] {
            let is_running = self
                .ssh_exec_capture(
                    instance,
                    &format!(
                        "systemctl is-active {} 2>/dev/null || echo inactive",
                        systemd_unit
                    ),
                )
                .map(|s| s.trim() == "active")
                .unwrap_or(false);

            services.push(ServiceStatus {
                name: service_name.to_string(),
                is_running,
                port: None, // Could extract from config
                host_port: None,
                metrics: None,
                error: None,
            });
        }

        Ok(services)
    }

    /// Get the uptime of the VM
    fn get_uptime(&self, instance: &str) -> Result<String> {
        self.ssh_exec_capture(instance, "uptime -p")
            .map(|s| s.trim().to_string())
    }

    /// Update the Vagrantfile to add or remove synced folders for temp VMs
    fn update_synced_folders(&self, vagrantfile: String, mounts: &[Mount]) -> Result<String> {
        let mut lines: Vec<String> = vagrantfile.lines().map(String::from).collect();
        let start_marker = "# BEGIN TEMP MOUNTS";
        let end_marker = "# END TEMP MOUNTS";

        // Remove the old temp mounts section if it exists
        if let Some(start_idx) = lines.iter().position(|l| l.contains(start_marker)) {
            if let Some(end_idx) = lines[start_idx..]
                .iter()
                .position(|l| l.contains(end_marker))
            {
                lines.drain(start_idx..=(start_idx + end_idx));
            }
        }

        // Prepare the new synced folder lines
        if !mounts.is_empty() {
            let mut new_mount_lines = vec![format!("  {}", start_marker)];
            for mount in mounts {
                let mount_type = match mount.permissions {
                    MountPermission::ReadWrite => "",
                    MountPermission::ReadOnly => ", mount_options: [\"ro\"]",
                };
                new_mount_lines.push(format!(
                    "  config.vm.synced_folder \"{}\", \"{}\"{}",
                    mount.source.display(),
                    mount.target.display(),
                    mount_type
                ));
            }
            new_mount_lines.push(format!("  {}", end_marker));

            // Find a good place to insert the new lines
            if let Some(insert_pos) = lines
                .iter()
                .rposition(|l| l.trim().starts_with("config.vm.synced_folder"))
            {
                lines.splice(insert_pos + 1..insert_pos + 1, new_mount_lines);
            } else if let Some(insert_pos) =
                lines.iter().position(|l| l.contains("Vagrant.configure"))
            {
                lines.splice(insert_pos + 1..insert_pos + 1, new_mount_lines);
            } else {
                return Err(VmError::Provider(
                    "Could not find a suitable location in Vagrantfile to add mounts.".to_string(),
                ));
            }
        }

        Ok(lines.join("\n"))
    }

    /// Detect the underlying Vagrant provider (e.g., virtualbox, vmware)
    fn detect_vagrant_provider(&self) -> Result<String> {
        // Read from .vagrant directory or Vagrantfile
        // For now, default to virtualbox as it's the most common
        Ok("virtualbox".to_string())
    }

    /// Parse the output of `vagrant global-status` to find the VM ID
    fn parse_vm_id(&self, global_status_output: &str, instance_name: &str) -> Result<String> {
        // Skip the header lines
        for line in global_status_output.lines().skip(2) {
            if line.contains(instance_name) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if !parts.is_empty() {
                    return Ok(parts[0].to_string());
                }
            }
        }

        Err(VmError::Provider(format!(
            "Could not find VM ID for instance: {}",
            instance_name
        )))
    }
}

impl Provider for VagrantProvider {
    fn name(&self) -> &'static str {
        "vagrant"
    }

    fn create(&self) -> Result<()> {
        let progress = ProgressReporter::new();
        let main_phase = progress.start_phase("Creating Vagrant Environment");

        // Check if VM already exists
        ProgressReporter::task(&main_phase, "Checking existing VM status...");
        let status_output = std::process::Command::new("vagrant")
            .env("VAGRANT_CWD", self.project_dir.join("providers/vagrant"))
            .args(["status", DEFAULT_MACHINE_NAME])
            .output();

        if let Ok(output) = status_output {
            let status_str = String::from_utf8_lossy(&output.stdout);
            if status_str.contains("running")
                || status_str.contains("poweroff")
                || status_str.contains("saved")
            {
                ProgressReporter::task(&main_phase, "VM already exists.");
                vm_warning!("Vagrant VM already exists.");
                vm_println!("To recreate, first run: vm destroy");
                ProgressReporter::finish_phase(&main_phase, "Skipped creation.");
                return Ok(());
            }
        }
        ProgressReporter::task(&main_phase, "No existing VM found.");

        // Start VM with full provisioning
        ProgressReporter::task(&main_phase, "Starting Vagrant VM with provisioning...");

        // Use run_vagrant_command which handles environment variables thread-safely
        let result = self.run_vagrant_command(&["up"]);

        if result.is_err() {
            ProgressReporter::task(&main_phase, "VM creation failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return result;
        }

        ProgressReporter::task(&main_phase, "VM created successfully.");
        ProgressReporter::finish_phase(&main_phase, "Environment ready.");

        vm_success!("Vagrant environment created successfully!");
        vm_println!("Use 'vm ssh' to connect to the VM");
        Ok(())
    }

    fn create_instance(&self, instance_name: &str) -> Result<()> {
        let progress = ProgressReporter::new();
        let main_phase = progress.start_phase(&format!(
            "Creating Vagrant Environment Instance '{}'",
            instance_name
        ));

        // Check if VM already exists with instance name
        ProgressReporter::task(&main_phase, "Checking existing VM instance status...");
        let status_output = std::process::Command::new("vagrant")
            .env("VAGRANT_CWD", self.project_dir.join("providers/vagrant"))
            .args(["status", instance_name])
            .output();

        if let Ok(output) = status_output {
            let status_str = String::from_utf8_lossy(&output.stdout);
            if status_str.contains("running")
                || status_str.contains("poweroff")
                || status_str.contains("saved")
            {
                ProgressReporter::task(&main_phase, "VM instance already exists.");
                vm_warning!("Vagrant VM instance '{}' already exists.", instance_name);
                vm_println!("To recreate, first run: vm destroy {}", instance_name);
                ProgressReporter::finish_phase(&main_phase, "Skipped creation.");
                return Ok(());
            }
        }
        ProgressReporter::task(&main_phase, "No existing VM instance found.");

        // Create multi-machine Vagrantfile for this instance
        ProgressReporter::task(&main_phase, "Creating Vagrantfile for instance...");
        let instance_manager = self.instance_manager();
        instance_manager.create_multi_machine_config(&[instance_name])?;

        // Start VM with full provisioning for specific instance
        ProgressReporter::task(
            &main_phase,
            &format!(
                "Starting Vagrant VM instance '{}' with provisioning...",
                instance_name
            ),
        );

        // Use run_vagrant_command which handles environment variables thread-safely
        let up_result = self.run_vagrant_command(&["up", instance_name, "--provision"]);
        if up_result.is_err() {
            ProgressReporter::task(&main_phase, "VM instance creation failed.");
            ProgressReporter::finish_phase(&main_phase, "Creation failed.");
            return up_result;
        }

        ProgressReporter::task(&main_phase, "VM instance created successfully.");
        ProgressReporter::finish_phase(&main_phase, "Environment ready.");

        vm_success!(
            "Vagrant environment instance '{}' created successfully!",
            instance_name
        );
        vm_println!(
            "Use 'vm ssh {}' to connect to the VM instance",
            instance_name
        );
        Ok(())
    }

    fn create_with_context(&self, _context: &ProviderContext) -> Result<()> {
        // ProviderContext support implemented:
        // - Global config is already passed through VM_CONFIG_JSON environment variable
        //   in run_vagrant_command(), so Vagrantfile has access to it
        // - Verbose mode could be added as an additional environment variable if needed
        //   in future enhancements
        // - No Vagrantfile regeneration needed as all context is passed via environment

        // Note: We don't use env::set_var here as it's thread-unsafe.
        // Instead, context should be passed through run_vagrant_command if needed.
        // For now, the existing VM_CONFIG_JSON passing is sufficient for context support.

        // Use the existing create() which already passes config via environment
        self.create()
    }

    fn create_instance_with_context(
        &self,
        instance_name: &str,
        _context: &ProviderContext,
    ) -> Result<()> {
        // ProviderContext support implemented via VM_CONFIG_JSON environment variable
        // passed by run_vagrant_command(). No additional changes needed.
        self.create_instance(instance_name)
    }

    fn start(&self, container: Option<&str>) -> Result<()> {
        self.run_vagrant_command_with_machine(&["up"], container)
    }

    fn stop(&self, container: Option<&str>) -> Result<()> {
        self.run_vagrant_command_with_machine(&["halt"], container)
    }

    fn destroy(&self, container: Option<&str>) -> Result<()> {
        self.run_vagrant_command_with_machine(&["destroy", "-f"], container)
    }

    fn ssh(&self, container: Option<&str>, relative_path: &Path) -> Result<()> {
        let vagrant_cwd = self.project_dir.join("providers/vagrant");
        let machine_name = self.resolve_machine_name(container)?;

        if relative_path.as_os_str().is_empty() || relative_path == Path::new(".") {
            // Simple SSH without path change
            if machine_name == DEFAULT_MACHINE_NAME {
                duct::cmd("vagrant", &["ssh"])
                    .env("VAGRANT_CWD", &vagrant_cwd)
                    .run()?;
            } else {
                duct::cmd("vagrant", &["ssh", &machine_name])
                    .env("VAGRANT_CWD", &vagrant_cwd)
                    .run()?;
            }
        } else {
            // SSH with directory change
            let workspace_path = self
                .config
                .project
                .as_ref()
                .and_then(|p| p.workspace_path.as_deref())
                .unwrap_or(DEFAULT_WORKSPACE_PATH);

            // Validate and calculate target directory (prevent path traversal)
            let target_path =
                SecurityValidator::validate_relative_path(relative_path, workspace_path).map_err(
                    |e| VmError::Internal(format!("Invalid path for SSH operation: {}", e)),
                )?;
            let target_dir = target_path.to_string_lossy();

            // Get shell from config (terminal.shell or default to bash)
            let shell = self
                .config
                .terminal
                .as_ref()
                .and_then(|t| t.shell.as_deref())
                .unwrap_or("bash");

            // Use safe argument passing - avoid shell interpolation by using printf and exec
            // This prevents injection even if target_dir or shell contain special characters
            let safe_cmd = "cd \"$1\" && exec \"$2\"".to_string();

            if machine_name == DEFAULT_MACHINE_NAME {
                duct::cmd(
                    "vagrant",
                    &["ssh", "-c", &safe_cmd, "--", &target_dir, shell],
                )
                .env("VAGRANT_CWD", &vagrant_cwd)
                .run()?;
            } else {
                duct::cmd(
                    "vagrant",
                    &[
                        "ssh",
                        &machine_name,
                        "-c",
                        &safe_cmd,
                        "--",
                        &target_dir,
                        shell,
                    ],
                )
                .env("VAGRANT_CWD", &vagrant_cwd)
                .run()?;
            }
        }
        Ok(())
    }

    fn exec(&self, container: Option<&str>, cmd: &[String]) -> Result<()> {
        // Safely escape each argument for shell execution
        let escaped_args: Vec<String> = cmd.iter().map(|arg| shell_escape(arg)).collect();
        let safe_cmd = escaped_args.join(" ");
        let machine_name = self.resolve_machine_name(container)?;

        if machine_name == DEFAULT_MACHINE_NAME {
            self.run_vagrant_command(&["ssh", "-c", &safe_cmd])
        } else {
            self.run_vagrant_command(&["ssh", &machine_name, "-c", &safe_cmd])
        }
    }

    fn logs(&self, container: Option<&str>) -> Result<()> {
        vm_println!("Showing service logs - Press Ctrl+C to stop...");
        let machine_name = self.resolve_machine_name(container)?;

        if machine_name == DEFAULT_MACHINE_NAME {
            self.run_vagrant_command(&[
                "ssh",
                "-c",
                "sudo journalctl -u postgresql -u redis-server -u mongod -f",
            ])
        } else {
            self.run_vagrant_command(&[
                "ssh",
                &machine_name,
                "-c",
                "sudo journalctl -u postgresql -u redis-server -u mongod -f",
            ])
        }
    }

    fn status(&self, container: Option<&str>) -> Result<()> {
        let report = self.get_status_report(container)?;
        info!("{:#?}", report);
        Ok(())
    }

    fn get_status_report(&self, container: Option<&str>) -> Result<VmStatusReport> {
        let instance_name = self.resolve_machine_name(container)?;

        // 1. Get VM running state via `vagrant status`
        let is_running = self.is_instance_running(&instance_name)?;

        if !is_running {
            return Ok(VmStatusReport {
                name: instance_name.clone(),
                provider: "vagrant".to_string(),
                is_running: false,
                ..Default::default()
            });
        }

        // 2. Get resource usage via SSH commands
        let resources = self.get_resource_usage(&instance_name)?;

        // 3. Get service status for known services
        let services = self.get_service_statuses(&instance_name)?;

        // 4. Get uptime
        let uptime = self.get_uptime(&instance_name).ok();

        Ok(VmStatusReport {
            name: instance_name,
            provider: "vagrant".to_string(),
            container_id: None, // Vagrant doesn't have container IDs
            is_running: true,
            uptime,
            resources,
            services,
        })
    }

    fn start_with_context(&self, container: Option<&str>, context: &ProviderContext) -> Result<()> {
        let instance_name = self.resolve_machine_name(container)?;

        // Regenerate Vagrantfile if context has config
        if let Some(global_config) = &context.global_config {
            info!("Regenerating Vagrantfile with updated global config");
            self.regenerate_vagrantfile(&instance_name, global_config)?;
        }

        // Now start with updated config
        self.start(Some(&instance_name))
    }

    fn restart_with_context(
        &self,
        container: Option<&str>,
        context: &ProviderContext,
    ) -> Result<()> {
        let instance_name = self.resolve_machine_name(container)?;

        // Regenerate Vagrantfile if context has config
        if let Some(global_config) = &context.global_config {
            info!("Regenerating Vagrantfile with updated global config");
            self.regenerate_vagrantfile(&instance_name, global_config)?;
        }

        // Now restart with updated config
        // Use `vagrant reload` to apply config changes
        self.restart(Some(&instance_name))
    }

    fn restart(&self, container: Option<&str>) -> Result<()> {
        // Use vagrant reload to restart
        self.run_vagrant_command_with_machine(&["reload"], container)
    }

    fn provision(&self, container: Option<&str>) -> Result<()> {
        // Use vagrant provision to re-run provisioning
        self.run_vagrant_command_with_machine(&["provision"], container)
    }

    fn list(&self) -> Result<()> {
        // Use vagrant global-status to list all VMs
        stream_command("vagrant", &["global-status"])
    }

    fn kill(&self, container: Option<&str>) -> Result<()> {
        let instance_name = self.resolve_machine_name(container)?;
        let instance_dir = self.get_instance_dir(&instance_name)?;

        warn!("Force killing Vagrant VM: {}", instance_name);

        // Try graceful halt first
        let halt_result = duct::cmd("vagrant", &["halt", "--force"])
            .dir(&instance_dir)
            .run();

        if halt_result.is_ok() {
            info!("VM halted gracefully.");
            return Ok(());
        }

        // If graceful halt fails, kill the underlying hypervisor process
        warn!("Graceful halt failed. Killing VM process forcefully.");

        let vm_id_output = duct::cmd("vagrant", &["global-status", "--prune"])
            .read()
            .map_err(|e| VmError::Provider(format!("Failed to get VM ID: {}", e)))?;

        let vm_id = self.parse_vm_id(&vm_id_output, &instance_name)?;

        match self.detect_vagrant_provider()?.as_str() {
            "virtualbox" => {
                duct::cmd("VBoxManage", &["controlvm", &vm_id, "poweroff"])
                    .run()
                    .map_err(|e| {
                        VmError::Provider(format!("Failed to kill VirtualBox VM: {}", e))
                    })?;
            }
            "vmware_desktop" | "vmware_fusion" => {
                duct::cmd("vmrun", &["stop", &vm_id, "hard"])
                    .run()
                    .map_err(|e| VmError::Provider(format!("Failed to kill VMware VM: {}", e)))?;
            }
            "hyperv" => {
                duct::cmd(
                    "powershell",
                    &["-Command", &format!("Stop-VM -Name '{}' -Force", vm_id)],
                )
                .run()
                .map_err(|e| VmError::Provider(format!("Failed to kill Hyper-V VM: {}", e)))?;
            }
            provider => {
                return Err(VmError::Provider(format!(
                    "Force kill not implemented for provider: {}",
                    provider
                )));
            }
        }

        info!("VM process killed forcefully.");
        Ok(())
    }

    fn get_sync_directory(&self) -> String {
        // Return workspace_path from config
        self.config
            .project
            .as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or(DEFAULT_WORKSPACE_PATH)
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

    fn as_temp_provider(&self) -> Option<&dyn TempProvider> {
        Some(self)
    }

    fn clone_box(&self) -> Box<dyn Provider> {
        Box::new(self.clone())
    }
}

impl TempProvider for VagrantProvider {
    fn update_mounts(&self, state: &TempVmState) -> Result<()> {
        let instance_dir = self.get_instance_dir(&state.container_name)?;
        let vagrantfile_path = instance_dir.join("Vagrantfile");

        let current_content = std::fs::read_to_string(&vagrantfile_path)
            .map_err(|e| VmError::Provider(format!("Failed to read Vagrantfile: {}", e)))?;

        let new_content = self.update_synced_folders(current_content, &state.mounts)?;

        std::fs::write(&vagrantfile_path, new_content)
            .map_err(|e| VmError::Provider(format!("Failed to write Vagrantfile: {}", e)))?;

        self.recreate_with_mounts(state)
    }

    fn recreate_with_mounts(&self, state: &TempVmState) -> Result<()> {
        let instance_dir = self.get_instance_dir(&state.container_name)?;
        info!("Reloading Vagrant VM to apply mount changes");
        duct::cmd("vagrant", &["reload"])
            .dir(instance_dir)
            .run()
            .map_err(|e| VmError::Provider(format!("Failed to reload VM: {}", e)))?;
        Ok(())
    }

    fn check_container_health(&self, container_name: &str) -> Result<bool> {
        if !self.is_instance_running(container_name)? {
            return Ok(false);
        }
        let ssh_test = self.ssh_exec_capture(container_name, "echo healthy");
        Ok(ssh_test.is_ok())
    }

    fn is_container_running(&self, container_name: &str) -> Result<bool> {
        self.is_instance_running(container_name)
    }
}
