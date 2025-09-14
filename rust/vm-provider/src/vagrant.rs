use crate::{Provider, error::ProviderError};
use anyhow::Result;
use std::path::Path;
use vm_config::config::VmConfig;
use crate::utils::{is_tool_installed, stream_command};

pub struct VagrantProvider {
    config: VmConfig,
}

impl VagrantProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("vagrant") {
            return Err(ProviderError::DependencyNotFound("Vagrant".to_string()).into());
        }
        Ok(Self { config })
    }
}

impl Provider for VagrantProvider {
    fn name(&self) -> &'static str {
        "vagrant"
    }

    fn create(&self) -> Result<()> {
        println!("(VagrantProvider) Creating VM...");
        // TODO: Translate logic from Vagrantfile and provider scripts
        stream_command("vagrant", &["up"])
    }

    fn start(&self) -> Result<()> {
        stream_command("vagrant", &["up"])
    }

    fn stop(&self) -> Result<()> {
        stream_command("vagrant", &["halt"])
    }

    fn destroy(&self) -> Result<()> {
        stream_command("vagrant", &["destroy", "-f"])
    }

    fn ssh(&self, _relative_path: &Path) -> Result<()> {
        // Vagrant ssh handles interactivity
        duct::cmd("vagrant", &["ssh"]).run()?;
        Ok(())
    }

    fn exec(&self, cmd: &[String]) -> Result<()> {
        let mut args = vec!["ssh", "-c"];
        let cmd_str = cmd.join(" ");
        args.push(&cmd_str);
        stream_command("vagrant", &args)
    }

    fn logs(&self) -> Result<()> {
        println!("Logs are not typically supported by the Vagrant provider in the same way as Docker.");
        println!("You can SSH into the machine and check system logs (e.g., /var/log/syslog).");
        Ok(())
    }

    fn status(&self) -> Result<()> {
        stream_command("vagrant", &["status"])
    }
}
