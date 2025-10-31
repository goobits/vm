// Standard library
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// External crates
use tera::Context as TeraContext;
use vm_core::error::{Result, VmError};

// Internal imports
use super::UserConfig;
use crate::resources;
use crate::BoxConfig;
use vm_config::config::VmConfig;

pub struct BuildOperations<'a> {
    pub config: &'a VmConfig,
    pub temp_dir: &'a PathBuf,
}

impl<'a> BuildOperations<'a> {
    pub fn new(config: &'a VmConfig, temp_dir: &'a PathBuf) -> Self {
        Self { config, temp_dir }
    }

    /// Get box configuration, parsing BoxSpec from vm.box field
    fn get_box_config(&self) -> Result<BoxConfig> {
        let base_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

        if let Some(vm_settings) = &self.config.vm {
            if let Some(box_spec) = vm_settings.get_box_spec() {
                return BoxConfig::parse_for_docker(&box_spec, &base_dir);
            }
        }

        // Default to ubuntu:24.04
        Ok(BoxConfig::DockerImage("ubuntu:24.04".to_string()))
    }

    /// Get the generated custom image name for Dockerfiles
    fn get_custom_image_name(&self) -> String {
        format!(
            "vm-custom-{}",
            self.config
                .project
                .as_ref()
                .and_then(|p| p.name.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("dev")
        )
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
    ///
    /// Returns a tuple of (build_context_path, base_image_name)
    pub fn prepare_build_context(&self) -> Result<(PathBuf, String)> {
        use super::command::DockerOps;
        use vm_core::vm_info;

        // Get box configuration
        let box_config = self.get_box_config()?;

        // Handle different box types
        let base_image = match &box_config {
            BoxConfig::DockerImage(image) => {
                // Pull Docker image from registry
                self.pull_image(image)?;
                image.clone()
            }
            BoxConfig::Dockerfile {
                path,
                context,
                args,
            } => {
                // Build from custom Dockerfile
                if !path.exists() {
                    return Err(VmError::NotFound(format!(
                        "Dockerfile not found: {}",
                        path.display()
                    )));
                }

                vm_info!("Building from custom Dockerfile: {}", path.display());

                // Build the image with a generated name
                let image_name = self.get_custom_image_name();

                // Pass build args from BoxSpec::Build variant
                DockerOps::build_custom_image(path, &image_name, context, args.as_ref())?;

                image_name
            }
            BoxConfig::Snapshot(name) => {
                // Load image from global snapshot
                use vm_core::vm_println;

                vm_println!("Loading base image from snapshot '@{}'...", name);

                // Check if snapshot exists in ~/.config/vm/snapshots/global/<name>/
                let home_dir = std::env::var("HOME")
                    .or_else(|_| std::env::var("USERPROFILE"))
                    .map_err(|_| {
                        VmError::Internal("Could not determine home directory".to_string())
                    })?;

                let snapshot_dir = PathBuf::from(home_dir)
                    .join(".config")
                    .join("vm")
                    .join("snapshots")
                    .join("global")
                    .join(name);

                if !snapshot_dir.exists() {
                    return Err(VmError::Config(format!(
                        "Snapshot '@{}' not found. Import it first with:\n  vm snapshot import <file>",
                        name
                    )));
                }

                // Load metadata to get image tag
                let metadata_path = snapshot_dir.join("metadata.json");
                if !metadata_path.exists() {
                    return Err(VmError::Config(format!(
                        "Snapshot '@{}' is corrupted (metadata.json not found)",
                        name
                    )));
                }

                let metadata_content = std::fs::read_to_string(&metadata_path).map_err(|e| {
                    VmError::Internal(format!("Failed to read metadata file: {}", e))
                })?;

                let metadata: serde_json::Value =
                    serde_json::from_str(&metadata_content).map_err(|e| {
                        VmError::Internal(format!("Failed to parse metadata.json: {}", e))
                    })?;

                // Get the image tag from first service (base image snapshot always has one service)
                let image_tag = metadata
                    .get("services")
                    .and_then(|s| s.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|svc| svc.get("image_tag"))
                    .and_then(|tag| tag.as_str())
                    .ok_or_else(|| {
                        VmError::Config(format!(
                            "Snapshot '@{}' is corrupted (image_tag not found in metadata)",
                            name
                        ))
                    })?;

                // Check if image is already loaded
                let image_exists = Command::new("docker")
                    .args(["image", "inspect", image_tag])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);

                if !image_exists {
                    vm_println!("  Image not loaded, loading from snapshot...");

                    // Load image from tar file
                    let image_file_path = snapshot_dir.join("images").join("base.tar");

                    if !image_file_path.exists() {
                        return Err(VmError::Config(format!(
                            "Snapshot '@{}' is corrupted (base.tar not found)",
                            name
                        )));
                    }

                    let load_output = Command::new("docker")
                        .args(["load", "-i", image_file_path.to_str().unwrap()])
                        .output()
                        .map_err(|e| {
                            VmError::Internal(format!("Failed to load Docker image: {}", e))
                        })?;

                    if !load_output.status.success() {
                        let stderr = String::from_utf8_lossy(&load_output.stderr);
                        return Err(VmError::Internal(format!(
                            "Failed to load Docker image from snapshot: {}",
                            stderr
                        )));
                    }

                    vm_println!("  ✓ Image loaded successfully");
                }

                image_tag.to_string()
            }
            _ => {
                return Err(VmError::Internal(
                    "Invalid box configuration for Docker provider".to_string(),
                ));
            }
        };

        // Now use base_image for the rest of the build process

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

        // Only generate Dockerfile from template if not using a custom Dockerfile
        // (Custom Dockerfiles are already built in the BoxConfig::Dockerfile match arm above)
        if !matches!(box_config, BoxConfig::Dockerfile { .. }) {
            let dockerfile_path = build_context.join("Dockerfile.generated");
            self.generate_dockerfile(&dockerfile_path)?;
        }

        // Copy vm-worktree.sh script to build context
        // The Dockerfile will COPY this into the container
        let worktree_script = include_str!("vm-worktree.sh");
        let worktree_script_path = build_context.join("vm-worktree.sh");
        fs::write(&worktree_script_path, worktree_script)?;

        Ok((build_context, base_image))
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
    ///
    /// # Arguments
    /// * `base_image` - The base image name (from prepare_build_context)
    pub fn gather_build_args(&self, base_image: &str) -> Vec<String> {
        let mut args = Vec::new();

        // Use the provided base image (already determined in prepare_build_context)
        args.push(format!("--build-arg=base_image={}", base_image));

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
