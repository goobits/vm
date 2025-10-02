use crate::{
    common::instance::{InstanceInfo, InstanceResolver},
    context::ProviderContext,
    progress::ProgressReporter,
    security::SecurityValidator,
    Provider, VmError,
};
use std::env;
use std::path::Path;
use vm_config::config::VmConfig;
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

pub struct VagrantProvider {
    config: VmConfig,
    project_dir: std::path::PathBuf,
}

impl VagrantProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("vagrant") {
            return Err(VmError::Dependency("Vagrant".into()).into());
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
            println!("{}", line?);
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
        // Vagrant doesn't support dynamic context-based configuration yet
        // Fall back to regular create() for now
        // TODO: Implement Vagrantfile regeneration based on context
        self.create()
    }

    fn create_instance_with_context(
        &self,
        instance_name: &str,
        _context: &ProviderContext,
    ) -> Result<()> {
        // Fall back to regular create_instance for now
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
        match container {
            Some(_) => {
                let machine_name = self.resolve_machine_name(container)?;
                self.run_vagrant_command(&["status", &machine_name])
            }
            None => {
                // Show all machines status
                self.run_vagrant_command(&["status"])
            }
        }
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
        // Note: Vagrant has no force-kill distinct from destroy.
        // This delegates to destroy with -f flag which forces the operation.
        self.destroy(container)
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
}
