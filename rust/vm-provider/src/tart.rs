use crate::{Provider, error::ProviderError};
use anyhow::Result;
use std::path::Path;
use vm_config::config::VmConfig;
use crate::utils::{is_tool_installed, stream_command};

pub struct TartProvider {
    config: VmConfig,
}

impl TartProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        if !is_tool_installed("tart") {
            return Err(ProviderError::DependencyNotFound("Tart".to_string()).into());
        }
        Ok(Self { config })
    }

    fn vm_name(&self) -> String {
        self.config.project.as_ref()
            .and_then(|p| p.name.as_deref())
            .unwrap_or("vm-project").to_string()
    }
}

impl Provider for TartProvider {
    fn name(&self) -> &'static str {
        "tart"
    }

    fn create(&self) -> Result<()> {
        println!("(TartProvider) Creating VM...");
        // TODO: Translate logic from tart-provider.sh
        // This will involve `tart clone` and `tart run` with --no-graphics
        let image = self.config.tart.as_ref().and_then(|t| t.image.as_deref()).unwrap_or("ghcr.io/cirruslabs/ubuntu-22.04");
        stream_command("tart", &["clone", image, &self.vm_name()])?;
        stream_command("tart", &["run", "--no-graphics", &self.vm_name()])
    }

    fn start(&self) -> Result<()> {
        stream_command("tart", &["run", "--no-graphics", &self.vm_name()])
    }

    fn stop(&self) -> Result<()> {
        stream_command("tart", &["stop", &self.vm_name()])
    }

    fn destroy(&self) -> Result<()> {
        stream_command("tart", &["delete", &self.vm_name()])
    }

    fn ssh(&self, _relative_path: &Path) -> Result<()> {
        duct::cmd("tart", &["ssh", &self.vm_name()]).run()?;
        Ok(())
    }

    fn exec(&self, cmd: &[String]) -> Result<()> {
        let vm = self.vm_name();
        let mut args = vec!["ssh", &vm, "--"];
        let cmd_strs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        args.extend_from_slice(&cmd_strs);
        stream_command("tart", &args)
    }

    fn logs(&self) -> Result<()> {
        println!("Logs can be found in ~/.tart/vms/{}/app.log", self.vm_name());
        Ok(())
    }

    fn status(&self) -> Result<()> {
        stream_command("tart", &["list"])
    }
}
