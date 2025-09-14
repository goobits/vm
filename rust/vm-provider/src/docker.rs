use crate::{Provider, error::ProviderError};
use anyhow::Result;
use std::path::Path;
use vm_config::config::VmConfig;

pub struct DockerProvider {
    config: VmConfig,
}

impl DockerProvider {
    pub fn new(config: VmConfig) -> Result<Self> {
        Ok(Self { config })
    }
}

impl Provider for DockerProvider {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn create(&self) -> Result<()> {
        println!("(DockerProvider) Creating VM...");
        // TODO: Implement logic from vm.sh docker_up()
        // 1. Generate Dockerfile and docker-compose.yml
        // 2. Run `docker-compose build`
        // 3. Run `docker-compose up -d`
        // 4. Run provisioning (Ansible)
        Ok(())
    }

    fn start(&self) -> Result<()> {
        println!("(DockerProvider) Starting VM...");
        // TODO: Implement logic from vm.sh docker_start()
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        println!("(DockerProvider) Stopping VM...");
        // TODO: Implement logic from vm.sh docker_halt()
        Ok(())
    }

    fn destroy(&self) -> Result<()> {
        println!("(DockerProvider) Destroying VM...");
        // TODO: Implement logic from vm.sh docker_destroy()
        Ok(())
    }

    fn ssh(&self, relative_path: &Path) -> Result<()> {
        println!("(DockerProvider) SSHing into VM at path: {:?}", relative_path);
        // TODO: Implement logic from vm.sh docker_ssh()
        Ok(())
    }

    fn exec(&self, cmd: &[&str]) -> Result<()> {
        println!("(DockerProvider) Executing command: {:?}", cmd);
        // TODO: Implement logic
        Ok(())
    }

    fn status(&self) -> Result<()> {
        println!("(DockerProvider) Getting status...");
        // TODO: Implement logic from vm.sh docker_status()
        Ok(())
    }
}
