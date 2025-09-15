use crate::{Provider, error::ProviderError, progress::ProgressReporter};
use anyhow::Result;
use std::path::Path;
use std::env;
use vm_config::config::VmConfig;
use crate::utils::{is_tool_installed, stream_command};

pub struct VagrantProvider {
    config: VmConfig,
    project_dir: std::path::PathBuf,
}

impl VagrantProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("vagrant") {
            return Err(ProviderError::DependencyNotFound("Vagrant".to_string()).into());
        }
        let project_dir = env::current_dir()?;
        Ok(Self { config, project_dir })
    }

    fn run_vagrant_command(&self, args: &[&str]) -> Result<()> {
        // Set VAGRANT_CWD to providers/vagrant directory
        let vagrant_cwd = self.project_dir.join("providers/vagrant");
        let config_json = self.config.to_json()?;

        // Set environment variables
        env::set_var("VAGRANT_CWD", vagrant_cwd);
        env::set_var("VM_PROJECT_DIR", &self.project_dir);
        env::set_var("VM_CONFIG_JSON", config_json);

        stream_command("vagrant", args)
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
        progress.task(&main_phase, "Checking existing VM status...");
        let status_output = std::process::Command::new("vagrant")
            .env("VAGRANT_CWD", self.project_dir.join("providers/vagrant"))
            .args(&["status", "default"])
            .output();

        if let Ok(output) = status_output {
            let status_str = String::from_utf8_lossy(&output.stdout);
            if status_str.contains("running") || status_str.contains("poweroff") || status_str.contains("saved") {
                progress.task(&main_phase, "VM already exists.");
                println!("⚠️  Vagrant VM already exists.");
                println!("To recreate, first run: vm destroy");
                progress.finish_phase(&main_phase, "Skipped creation.");
                return Ok(());
            }
        }
        progress.task(&main_phase, "No existing VM found.");

        // Start VM with full provisioning
        progress.task(&main_phase, "Starting Vagrant VM with provisioning...");
        let vagrant_cwd = self.project_dir.join("providers/vagrant");
        let config_json = self.config.to_json()?;

        env::set_var("VAGRANT_CWD", &vagrant_cwd);
        env::set_var("VM_PROJECT_DIR", &self.project_dir);
        env::set_var("VM_CONFIG_JSON", config_json);

        let result = stream_command("vagrant", &["up"]);

        if result.is_err() {
            progress.task(&main_phase, "VM creation failed.");
            progress.finish_phase(&main_phase, "Creation failed.");
            return result;
        }

        progress.task(&main_phase, "VM created successfully.");
        progress.finish_phase(&main_phase, "Environment ready.");

        println!("\n✅ Vagrant environment created successfully!");
        println!("💡 Use 'vm ssh' to connect to the VM");
        Ok(())
    }

    fn start(&self) -> Result<()> {
        self.run_vagrant_command(&["resume"]).or_else(|_| {
            self.run_vagrant_command(&["up"])
        })
    }

    fn stop(&self) -> Result<()> {
        self.run_vagrant_command(&["halt"])
    }

    fn destroy(&self) -> Result<()> {
        self.run_vagrant_command(&["destroy", "-f"])
    }

    fn ssh(&self, relative_path: &Path) -> Result<()> {
        let vagrant_cwd = self.project_dir.join("providers/vagrant");
        env::set_var("VAGRANT_CWD", &vagrant_cwd);

        if relative_path.as_os_str().is_empty() || relative_path == Path::new(".") {
            // Simple SSH without path change
            duct::cmd("vagrant", &["ssh"]).run()?;
        } else {
            // SSH with directory change
            let workspace_path = self.config.project.as_ref()
                .and_then(|p| p.workspace_path.as_deref())
                .unwrap_or("/workspace");
            let target_dir = format!("{}/{}", workspace_path, relative_path.display());
            // Get shell from config (terminal.shell or default to bash)
            let shell = self.config.terminal.as_ref()
                .and_then(|t| t.shell.as_deref())
                .unwrap_or("bash");
            let cmd = format!("cd {} && exec {}", target_dir, shell);

            duct::cmd("vagrant", &["ssh", "-c", &cmd]).run()?;
        }
        Ok(())
    }

    fn exec(&self, cmd: &[String]) -> Result<()> {
        let cmd_str = cmd.join(" ");
        self.run_vagrant_command(&["ssh", "-c", &cmd_str])
    }

    fn logs(&self) -> Result<()> {
        println!("Showing service logs - Press Ctrl+C to stop...");
        self.run_vagrant_command(&[
            "ssh", "-c",
            "sudo journalctl -u postgresql -u redis-server -u mongod -f"
        ])
    }

    fn status(&self) -> Result<()> {
        self.run_vagrant_command(&["status"])
    }

    fn restart(&self) -> Result<()> {
        // Use vagrant reload to restart
        self.run_vagrant_command(&["reload"])
    }

    fn provision(&self) -> Result<()> {
        // Use vagrant provision to re-run provisioning
        self.run_vagrant_command(&["provision"])
    }

    fn list(&self) -> Result<()> {
        // Use vagrant global-status to list all VMs
        stream_command("vagrant", &["global-status"])
    }

    fn kill(&self) -> Result<()> {
        // Force destroy the VM
        self.run_vagrant_command(&["destroy", "-f"])
    }

    fn get_sync_directory(&self) -> Result<String> {
        // Return workspace_path from config
        let workspace_path = self.config.project.as_ref()
            .and_then(|p| p.workspace_path.as_deref())
            .unwrap_or("/workspace");
        Ok(workspace_path.to_string())
    }
}
