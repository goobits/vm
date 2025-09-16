// Standard library
use std::fs;
use std::path::{Path, PathBuf};

// External crates
use anyhow::Result;
use tera::{Context as TeraContext, Tera};

// Internal imports
use crate::resources;
use super::UserConfig;
use vm_config::config::VmConfig;

pub struct BuildOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a PathBuf,
}

impl<'a> BuildOperations<'a> {
    pub fn new(config: &'a VmConfig, temp_dir: &'a PathBuf) -> Self {
        Self { config, temp_dir }
    }

    /// Safely convert a path to string with descriptive error message
    pub fn path_to_string(path: &Path) -> Result<&str> {
        path.to_str().ok_or_else(|| {
            anyhow::anyhow!("Path contains invalid UTF-8 characters: {}", path.display())
        })
    }

    /// Prepare build context with embedded resources and generated Dockerfile
    pub fn prepare_build_context(&self) -> Result<PathBuf> {
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
        let mut tera = Tera::default();
        let template_content = include_str!("Dockerfile.j2");
        tera.add_raw_template("Dockerfile", template_content)?;

        let user_config = UserConfig::from_vm_config(self.config);

        let mut context = TeraContext::new();
        context.insert("project_uid", &user_config.uid.to_string());
        context.insert("project_gid", &user_config.gid.to_string());
        context.insert("project_user", &user_config.username);

        let content = tera.render("Dockerfile", &context)?;
        fs::write(output_path, content.as_bytes())?;

        Ok(())
    }

    /// Gather all package lists and format as build arguments
    pub fn gather_build_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // Add version build args
        if let Some(versions) = &self.config.versions {
            if let Some(node) = &versions.node {
                args.push(format!("--build-arg=NODE_VERSION={}", node));
            }
            if let Some(nvm) = &versions.nvm {
                args.push(format!("--build-arg=NVM_VERSION={}", nvm));
            }
            if let Some(pnpm) = &versions.pnpm {
                args.push(format!("--build-arg=PNPM_VERSION={}", pnpm));
            }
        }

        // Add package list build args
        if !self.config.apt_packages.is_empty() {
            let packages = self.config.apt_packages.join(" ");
            args.push(format!("--build-arg=APT_PACKAGES={}", packages));
        }

        if !self.config.npm_packages.is_empty() {
            let packages = self.config.npm_packages.join(" ");
            args.push(format!("--build-arg=NPM_PACKAGES={}", packages));
        }

        if !self.config.pip_packages.is_empty() {
            let packages = self.config.pip_packages.join(" ");
            args.push(format!("--build-arg=PIP_PACKAGES={}", packages));
        }

        if !self.config.pipx_packages.is_empty() {
            let packages = self.config.pipx_packages.join(" ");
            args.push(format!("--build-arg=PIPX_PACKAGES={}", packages));
        }

        if !self.config.cargo_packages.is_empty() {
            let packages = self.config.cargo_packages.join(" ");
            args.push(format!("--build-arg=CARGO_PACKAGES={}", packages));
        }

        // Add user/group build args
        let user_config = UserConfig::from_vm_config(self.config);

        args.push(format!("--build-arg=PROJECT_UID={}", user_config.uid));
        args.push(format!("--build-arg=PROJECT_GID={}", user_config.gid));
        args.push(format!("--build-arg=PROJECT_USER={}", user_config.username));

        args
    }
}
