use std::{fs::File, path::PathBuf, process::Stdio};

use tracing::{info, warn};
use vm_config::config::{ImageSpec, VmConfig};
use vm_core::error::Result;
use vm_messages::messages::MESSAGES;

use super::{
    host_sync::collect_host_sync_mounts,
    mounts::TartDirShare,
    provider::{tart_run_log_path, TartProvider},
    provisioner::TartProvisioner,
};
use crate::{instance::extract_project_name, project_plan::ProjectPlan, tart_base, VmError};

const DEFAULT_TART_IMAGE: &str = "ghcr.io/cirruslabs/macos-sequoia-base:latest";

#[derive(Debug)]
enum TartImageSource {
    Image(String),
    Snapshot(String),
}

impl TartImageSource {
    fn parse(spec: &ImageSpec) -> Result<Self> {
        match spec {
            ImageSpec::String(value) => {
                if let Some(name) = value.strip_prefix('@') {
                    return Ok(Self::Snapshot(name.to_string()));
                }
                let lower = value.to_ascii_lowercase();
                if value.starts_with("./")
                    || value.starts_with("../")
                    || std::path::Path::new(value).is_absolute()
                    || lower == "dockerfile"
                    || lower.ends_with("/dockerfile")
                    || lower.ends_with(".dockerfile")
                {
                    return Err(VmError::Config(format!(
                        "'{value}' looks like a Dockerfile path, but the Tart provider cannot build Dockerfiles. Use provider: docker or choose a Tart OCI image."
                    )));
                }
                Ok(Self::Image(value.clone()))
            }
            ImageSpec::Build { .. } => Err(VmError::Config(
                "Tart provider does not support Dockerfile builds".to_string(),
            )),
        }
    }
}

impl TartProvider {
    pub(super) fn start_vm_background(&self, name: &str) -> Result<()> {
        self.start_vm_background_with_dir_shares(name, &[])
    }

    pub(super) fn start_vm_background_with_dir_shares(
        &self,
        name: &str,
        extra: &[TartDirShare],
    ) -> Result<()> {
        let log_path = tart_run_log_path(name);
        info!("Tart run log for '{}': {}", name, log_path);
        let workspace = TartDirShare {
            tag: "workspace".to_string(),
            host_path: self.host_workspace_path()?,
            guest_path: Some(PathBuf::from(self.effective_sync_directory())),
            access: self
                .config
                .project
                .as_ref()
                .map(|project| project.workspace_access)
                .unwrap_or_default(),
        };
        let mut directories = vec![workspace.tart_argument()];
        directories.extend(
            collect_host_sync_mounts(&self.config)
                .into_iter()
                .map(|mount| format!("{}:tag={}", mount.host_path.display(), mount.tag)),
        );
        directories.extend(
            self.configured_dir_shares()?
                .into_iter()
                .map(|share| share.tart_argument()),
        );
        directories.extend(extra.iter().map(TartDirShare::tart_argument));
        let stdout = File::create(&log_path).map_err(|error| {
            VmError::Provider(format!("Failed to create Tart run log: {error}"))
        })?;
        let stderr = stdout
            .try_clone()
            .map_err(|error| VmError::Provider(format!("Failed to open Tart run log: {error}")))?;
        let mut command = std::process::Command::new("nohup");
        self.command.configure(&mut command);
        command.args(self.build_run_args(name, &directories));
        command
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .map_err(|error| {
                VmError::Provider(format!("Failed to start Tart VM: {error}. See {log_path}"))
            })?;
        Ok(())
    }

    pub(super) fn build_run_args(&self, name: &str, directories: &[String]) -> Vec<String> {
        let tart = self.config.tart.as_ref();
        let nested = tart.and_then(|config| config.nested).unwrap_or(false)
            && tart.and_then(|config| config.guest_os.as_deref()) != Some("macos");
        let mut args = vec![
            "tart".to_string(),
            "run".to_string(),
            "--no-graphics".to_string(),
        ];
        if nested {
            args.push("--nested".to_string());
        }
        for directory in directories {
            args.extend(["--dir".to_string(), directory.clone()]);
        }
        args.push(name.to_string());
        args
    }

    pub(super) fn get_tart_image(&self, config: &VmConfig) -> Result<String> {
        if let Some(image_spec) = config
            .vm
            .as_ref()
            .and_then(|settings| settings.image.clone())
        {
            return match TartImageSource::parse(&image_spec)? {
                TartImageSource::Image(image) if image == tart_base::LINUX_NAME => {
                    Ok(tart_base::versioned_cache_name())
                }
                TartImageSource::Image(image) => Ok(image),
                TartImageSource::Snapshot(name) => Err(VmError::Config(format!(
                    "Use 'vm revert {name}' for snapshots"
                ))),
            };
        }
        Ok(DEFAULT_TART_IMAGE.to_string())
    }

    pub(super) fn create_vm_internal(
        &self,
        name: &str,
        label: Option<&str>,
        config: &VmConfig,
    ) -> Result<()> {
        self.create_vm_internal_with_dir_shares(name, label, config, &[])
    }

    pub(super) fn create_vm_internal_with_dir_shares(
        &self,
        name: &str,
        label: Option<&str>,
        config: &VmConfig,
        extra: &[TartDirShare],
    ) -> Result<()> {
        let operation = label.map_or_else(
            || "Creating Tart VM".to_string(),
            |label| format!("Creating Tart VM instance '{label}'"),
        );
        info!("{operation}");
        info!("Checking whether Tart VM exists");
        if self.get_instance_state(name)?.is_some() {
            return Err(VmError::Conflict(format!(
                "Tart VM '{name}' already exists"
            )));
        }
        let prefix = format!("{}-", extract_project_name(&self.config));
        let orphans = self
            .instance_manager()
            .parse_tart_list()?
            .into_iter()
            .map(|instance| instance.name)
            .filter(|candidate| candidate.starts_with(&prefix) && candidate != name)
            .collect::<Vec<_>>();
        if !orphans.is_empty() {
            warn!("Found potential orphaned VMs from previous runs/instances");
            warn!(
                "Other Tart environments exist for this project: {}",
                orphans.join(", ")
            );
        }
        let image = self.get_tart_image(config)?;
        if (image == tart_base::MACOS_NAME || image == tart_base::versioned_cache_name())
            && !self.tart_image_exists(&image)?
        {
            return Err(VmError::Config(format!("Tart vibe base '{image}' was not found. Run `vm system base build vibe --provider tart` first.")));
        }
        info!("Cloning Tart image '{image}'");
        self.stream_tart_command(&["clone", &image, name])?;
        self.command.remember_instance(name)?;
        let resources = Self::resolved_tart_resources(config)?;
        if let Some(memory) = resources.memory_mb {
            info!("Setting Tart memory to {memory} MB");
            self.stream_tart_command(&["set", name, "--memory", &memory.to_string()])?;
        }
        if let Some(cpus) = resources.cpus {
            info!("Setting Tart CPUs to {cpus}");
            self.stream_tart_command(&["set", name, "--cpu", &cpus.to_string()])?;
        }
        if let Some(disk) = config
            .tart
            .as_ref()
            .and_then(|config| config.disk_size.as_ref())
            .and_then(|limit| limit.to_gb())
        {
            info!("Setting Tart disk size to {disk} GB");
            self.stream_tart_command(&["set", name, "--disk-size", &disk.to_string()])?;
        }
        info!("Starting Tart VM");
        self.start_vm_background_with_dir_shares(name, extra)?;
        info!("Running initial Tart provisioning");
        let provisioner = TartProvisioner::new(
            name.to_string(),
            self.effective_sync_directory(),
            self.command.clone(),
        );
        let plan = ProjectPlan::detect(&self.host_workspace_path()?, config);
        if let Err(error) = provisioner.provision(config, &plan) {
            return Err(VmError::Provider(format!(
                "{error}. Tart run log: {}",
                tart_run_log_path(name)
            )));
        }
        let mut shares = self.configured_dir_shares()?;
        shares.extend_from_slice(extra);
        if !shares.is_empty() {
            info!("Mounting Tart shared directories");
            self.mount_tart_dir_shares_in_guest(name, &shares)?;
        }
        info!("Tart environment is ready");
        info!("{}", MESSAGES.service.provider_tart_created_success);
        info!("{}", MESSAGES.service.provider_tart_connect_hint);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::TartImageSource;
    use vm_config::config::ImageSpec;

    #[test]
    fn parses_tart_images_and_snapshots() {
        assert!(matches!(
            TartImageSource::parse(&ImageSpec::String("ghcr.io/example/tart:latest".into()))
                .unwrap(),
            TartImageSource::Image(image) if image == "ghcr.io/example/tart:latest"
        ));
        assert!(matches!(
            TartImageSource::parse(&ImageSpec::String("@release".into())).unwrap(),
            TartImageSource::Snapshot(name) if name == "release"
        ));
    }

    #[test]
    fn rejects_dockerfile_sources() {
        for value in [
            "Dockerfile",
            "./Dockerfile",
            "../Dockerfile",
            "build.dev.dockerfile",
        ] {
            let error = TartImageSource::parse(&ImageSpec::String(value.into())).unwrap_err();
            assert!(error.to_string().contains("looks like a Dockerfile path"));
        }
        assert!(TartImageSource::parse(&ImageSpec::Build {
            dockerfile: "Dockerfile".into(),
            context: None,
            args: None,
        })
        .is_err());
    }
}
