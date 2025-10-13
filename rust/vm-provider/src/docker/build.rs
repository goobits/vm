// Standard library
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// External crates
use tera::Context as TeraContext;
use vm_core::error::{Result, VmError};

// Internal imports
use super::UserConfig;
use crate::resources;
use vm_config::config::VmConfig;

pub struct BuildOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a PathBuf,
}

impl<'a> BuildOperations<'a> {
    pub fn new(config: &'a VmConfig, temp_dir: &'a PathBuf) -> Self {
        Self { config, temp_dir }
    }

    pub fn pull_image(&self, image: &str) -> Result<()> {
        let output = Command::new("docker").args(["pull", image]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            // Detect rate limiting
            if stderr.contains("toomanyrequests") || stderr.contains("rate limit") {
                return Err(VmError::Internal(
                    "Docker Hub rate limit reached\n\n\
                    Fixes:\n\
                      • Wait 6 hours and try again\n\
                      • Login to Docker Hub: docker login"
                        .to_string(),
                ));
            }

            return Err(VmError::Internal(format!(
                "Docker pull failed for image '{image}': {stderr}"
            )));
        }

        Ok(())
    }

    /// Safely convert a path to string with descriptive error message
    pub fn path_to_string(path: &Path) -> Result<&str> {
        path.to_str().ok_or_else(|| {
            VmError::Internal(format!(
                "Path '{}' contains invalid UTF-8 characters and cannot be used as Docker build argument",
                path.display()
            ))
        })
    }

    /// Prepare build context with embedded resources and generated Dockerfile
    pub fn prepare_build_context(&self) -> Result<PathBuf> {
        let image_to_pull = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.box_name.as_deref())
            .unwrap_or("ubuntu:24.04");
        self.pull_image(image_to_pull)?;

        // Create temporary build context directory
        let build_context = self.temp_dir.join("build_context");
        if build_context.exists() {
            fs::remove_dir_all(&build_context)?;
        }
        fs::create_dir_all(&build_context)?;

        // Create shared directory and copy embedded resources
        let shared_dir = build_context.join("shared");
        fs::create_dir_all(&shared_dir)?;

        // Copy embedded resources to build context
        resources::copy_embedded_resources(&shared_dir)?;

        // Generate Dockerfile from template
        let dockerfile_path = build_context.join("Dockerfile.generated");
        self.generate_dockerfile(&dockerfile_path)?;

        Ok(build_context)
    }

    /// Generate Dockerfile from template with build args
    pub fn generate_dockerfile(&self, output_path: &Path) -> Result<()> {
        // Use shared template engine instead of creating new instance
        let tera = super::get_dockerfile_tera();

        let user_config = self.get_user_config();

        let mut context = TeraContext::new();
        context.insert("project_uid", &user_config.uid.to_string());
        context.insert("project_gid", &user_config.gid.to_string());
        context.insert("project_user", &user_config.username);

        let content = tera
            .render("Dockerfile", &context)
            .map_err(|e| VmError::Internal(format!("Failed to render Dockerfile template: {e}")))?;
        fs::write(output_path, content.as_bytes())?;

        Ok(())
    }

    /// Gather all package lists and format as build arguments
    pub fn gather_build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        if let Some(image) = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.box_name.as_deref())
        {
            args.push(format!("--build-arg=base_image={image}"));
        }

        // Add version build args
        if let Some(versions) = &self.config.versions {
            if let Some(node) = &versions.node {
                args.push(format!("--build-arg=NODE_VERSION={node}"));
            }
            if let Some(nvm) = &versions.nvm {
                args.push(format!("--build-arg=NVM_VERSION={nvm}"));
            }
            if let Some(pnpm) = &versions.pnpm {
                args.push(format!("--build-arg=PNPM_VERSION={pnpm}"));
            }
        }

        // Add package list build args
        if !self.config.apt_packages.is_empty() {
            let packages = self.config.apt_packages.join(" ");
            args.push(format!("--build-arg=APT_PACKAGES={packages}"));
        }

        if !self.config.npm_packages.is_empty() {
            let packages = self.config.npm_packages.join(" ");
            args.push(format!("--build-arg=NPM_PACKAGES={packages}"));
        }

        if !self.config.pip_packages.is_empty() {
            let packages = self.config.pip_packages.join(" ");
            args.push(format!("--build-arg=PIP_PACKAGES={packages}"));
        }

        if !self.config.cargo_packages.is_empty() {
            let packages = self.config.cargo_packages.join(" ");
            args.push(format!("--build-arg=CARGO_PACKAGES={packages}"));
        }

        // Add user/group build args
        let user_config = self.get_user_config();

        args.push(format!("--build-arg=PROJECT_UID={}", user_config.uid));
        args.push(format!("--build-arg=PROJECT_GID={}", user_config.gid));
        args.push(format!("--build-arg=PROJECT_USER={}", user_config.username));

        // Add timezone build arg
        if let Some(timezone) = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.timezone.as_deref())
        {
            args.push(format!("--build-arg=TZ={}", timezone));
        }

        // Add git config build args
        if let Some(git_config) = &self.config.git_config {
            if let Some(name) = &git_config.user_name {
                args.push(format!("--build-arg=GIT_USER_NAME={}", name));
            }
            if let Some(email) = &git_config.user_email {
                args.push(format!("--build-arg=GIT_USER_EMAIL={}", email));
            }
            if let Some(rebase) = &git_config.pull_rebase {
                args.push(format!("--build-arg=GIT_PULL_REBASE={}", rebase));
            }
            if let Some(branch) = &git_config.init_default_branch {
                args.push(format!("--build-arg=GIT_INIT_DEFAULT_BRANCH={}", branch));
            }
            if let Some(editor) = &git_config.core_editor {
                args.push(format!("--build-arg=GIT_CORE_EDITOR={}", editor));
            }
            if let Some(content) = &git_config.core_excludesfile_content {
                args.push(format!(
                    "--build-arg=GIT_CORE_EXCLUDESFILE_CONTENT={}",
                    content
                ));
            }
        }

        args
    }

    /// Get user configuration from VM config
    ///
    /// Centralizes the creation of UserConfig to avoid duplication and ensure consistency.
    fn get_user_config(&self) -> UserConfig {
        UserConfig::from_vm_config(self.config)
    }
}
