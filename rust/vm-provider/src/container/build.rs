// Standard library
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

// External crates
use sha2::{Digest, Sha256};
use tera::Context as TeraContext;
use vm_core::error::{Result, VmError};

// Internal imports
use super::{compose_context::managed_worktree_root, UserConfig};
use crate::BoxConfig;
use crate::{project_plan::NodeToolchainPlan, resources};
use vm_config::config::VmConfig;

pub struct BuildOperations<'a> {
    pub config: &'a VmConfig,
    pub generated_dir: &'a Path,
    pub executable: &'a str,
}

impl<'a> BuildOperations<'a> {
    pub fn new(config: &'a VmConfig, generated_dir: &'a Path, executable: &'a str) -> Self {
        Self {
            config,
            generated_dir,
            executable,
        }
    }

    pub fn build_context_dir(&self) -> PathBuf {
        self.generated_dir.join("build_context")
    }

    /// Prepare the reusable build context needed for compose/build.
    ///
    /// This avoids destructive rebuilds of the build context for start/restart flows.
    pub fn prepare_compose_build_context(&self) -> Result<PathBuf> {
        let build_context = self.build_context_dir();
        fs::create_dir_all(&build_context)?;

        let dockerignore_content = r#"# Git and version control
.git
.gitignore
.github

# Build artifacts and dependencies
node_modules
target
dist
build
*.log

# IDE and editor files
.vscode
.idea
*.swp
*.swo
*~

# OS files
.DS_Store
Thumbs.db

# Temporary files
*.tmp
*.bak
.cache
"#;
        Self::write_if_changed(&build_context.join(".dockerignore"), dockerignore_content)?;

        // Create shared directory and copy embedded resources
        let shared_dir = build_context.join("shared");
        fs::create_dir_all(&shared_dir)?;

        // Copy embedded resources to build context
        resources::copy_embedded_resources(&shared_dir)?;

        // Generate Dockerfile from template
        // For custom Dockerfiles, generate a minimal wrapper that uses the pre-built image
        let dockerfile_path = build_context.join("Dockerfile.generated");
        if matches!(self.get_box_config()?, BoxConfig::Dockerfile { .. }) {
            // Custom Dockerfile case: Generate minimal Dockerfile that uses the pre-built image
            self.generate_dockerfile_from_image(&dockerfile_path, &self.get_custom_image_name())?;
        } else {
            // Standard case: Generate full Dockerfile from template
            self.generate_dockerfile(&dockerfile_path)?;
        }

        // Copy vm-worktree.sh script to build context
        // The Dockerfile will COPY this into the container
        let worktree_script = include_str!("vm-worktree.sh");
        let worktree_script_path = build_context.join("vm-worktree.sh");
        Self::write_if_changed(&worktree_script_path, worktree_script)?;

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

        // Keep the image's shell helper aligned with the runtime worktree mount.
        context.insert(
            "worktrees_base_dir",
            &managed_worktree_root(self.config).to_string_lossy(),
        );

        let content = tera
            .render("Dockerfile", &context)
            .map_err(|e| VmError::Internal(format!("Failed to render Dockerfile template: {e}")))?;
        Self::write_if_changed(output_path, &content)?;

        Ok(())
    }

    /// Generate minimal Dockerfile that uses a pre-built custom image as base
    ///
    /// This is used when --from-dockerfile is specified. The custom Dockerfile has already
    /// been built into an image, so we just need a minimal wrapper Dockerfile that:
    /// 1. Uses FROM the pre-built custom image
    /// 2. Copies shared resources (shell prompt, etc.)
    pub fn generate_dockerfile_from_image(
        &self,
        output_path: &Path,
        base_image: &str,
    ) -> Result<()> {
        let user_config = self.get_user_config();

        let content = format!(
            r#"# Generated Dockerfile wrapper for custom base image
FROM {base_image}

LABEL com.vm.managed="true"

ARG PROJECT_UID={uid}
ARG PROJECT_GID={gid}
ARG PROJECT_USER={user}

# Switch to root temporarily to copy system resources
USER root

# Copy shared resources (ansible playbooks, services, templates)
COPY shared/ /app/shared/

# Copy git worktree helper script with executable permissions
COPY --chmod=755 vm-worktree.sh /usr/local/bin/vm-worktree

# Switch back to the project user (if the base image set one)
USER ${user}

# Set working directory
WORKDIR /workspace

# Keep container running
CMD ["tail", "-f", "/dev/null"]
"#,
            base_image = base_image,
            uid = user_config.uid,
            gid = user_config.gid,
            user = user_config.username,
        );

        Self::write_if_changed(output_path, &content)?;
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

        // Detect if using a pre-provisioned snapshot to skip redundant base provisioning
        // Use explicit BoxConfig check instead of string matching to avoid false positives
        // (e.g., "company/dev-box:latest" should NOT be treated as a snapshot)
        let is_snapshot = self.uses_preprovisioned_snapshot();

        args.push(format!("--build-arg=BASE_PREPROVISIONED={}", is_snapshot));
        if self.uses_vibe_snapshot() {
            args.push("--build-arg=VIBE_RUNTIME_REQUIRED=true".to_string());
        }

        // Resolve Node defaults once for build-time and runtime provisioning.
        let node = NodeToolchainPlan::resolve(self.config);
        args.push(format!("--build-arg=NODE_VERSION={}", node.node));
        args.push(format!("--build-arg=NVM_VERSION={}", node.nvm));
        args.push(format!("--build-arg=PNPM_VERSION={}", node.pnpm));
        if let Some(npm) = node.npm {
            args.push(format!("--build-arg=NPM_VERSION={npm}"));
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

        // Snapshot-based creates should maximize reuse of the pre-provisioned baseline.
        // Avoid feeding host-specific identity values into the Docker build in that path,
        // because they force unnecessary cache misses across machines/users.
        if !is_snapshot {
            let user_config = self.get_user_config();
            args.push(format!("--build-arg=PROJECT_UID={}", user_config.uid));
            args.push(format!("--build-arg=PROJECT_GID={}", user_config.gid));
            args.push(format!("--build-arg=PROJECT_USER={}", user_config.username));
        }

        // Add timezone build arg
        if let Some(timezone) = self
            .config
            .vm
            .as_ref()
            .and_then(|vm| vm.timezone.as_deref())
        {
            args.push(format!("--build-arg=TZ={}", timezone));
        }

        // Apply host Git identity at runtime for snapshot-based creates to preserve cache reuse.
        if !is_snapshot {
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
        }

        args
    }

    pub fn uses_preprovisioned_snapshot(&self) -> bool {
        self.get_box_config()
            .map(|cfg| matches!(cfg, BoxConfig::Snapshot(_)))
            .unwrap_or(false)
    }

    fn uses_vibe_snapshot(&self) -> bool {
        self.get_box_config()
            .is_ok_and(|config| matches!(config, BoxConfig::Snapshot(name) if name == "vibe-box"))
    }

    pub fn image_exists(&self, image: &str) -> Result<bool> {
        let inspect = Command::new(self.executable)
            .args(["image", "inspect", image])
            .output()?;
        Ok(inspect.status.success())
    }

    pub fn image_identity(&self, image: &str) -> Result<String> {
        let inspect = Command::new(self.executable)
            .args(["image", "inspect", "--format", "{{.Id}}", image])
            .output()?;
        if !inspect.status.success() {
            return Err(VmError::Internal(format!(
                "Failed to inspect base image '{}': {}",
                image,
                String::from_utf8_lossy(&inspect.stderr).trim()
            )));
        }

        let identity = String::from_utf8_lossy(&inspect.stdout).trim().to_string();
        if identity.is_empty() {
            return Err(VmError::Internal(format!(
                "Base image '{image}' did not report an image ID"
            )));
        }
        Ok(identity)
    }

    pub fn derived_image_tag(
        &self,
        base_image: &str,
        base_image_identity: &str,
        build_context: &Path,
    ) -> Result<String> {
        let mut hasher = Sha256::new();
        hasher.update(base_image.as_bytes());
        hasher.update([0]);
        hasher.update(base_image_identity.as_bytes());
        hasher.update([0]);

        for arg in self.gather_build_args(base_image) {
            hasher.update(arg.as_bytes());
            hasher.update([0]);
        }

        Self::hash_build_context(build_context, build_context, &mut hasher)?;

        use std::fmt::Write as _;
        let digest = hasher
            .finalize()
            .iter()
            .fold(String::new(), |mut acc, byte| {
                let _ = write!(acc, "{byte:02x}");
                acc
            });
        let short_digest = &digest[..16];
        Ok(format!("vm-derived:{short_digest}"))
    }

    fn hash_build_context(root: &Path, path: &Path, hasher: &mut Sha256) -> Result<()> {
        let metadata = fs::symlink_metadata(path)?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");

        if metadata.is_dir() {
            hasher.update(b"dir:");
            hasher.update(relative.as_bytes());
            hasher.update([0]);

            let mut entries: Vec<_> = fs::read_dir(path)?.collect::<std::result::Result<_, _>>()?;
            entries.sort_by_key(|entry| entry.file_name());

            for entry in entries {
                Self::hash_build_context(root, &entry.path(), hasher)?;
            }
            return Ok(());
        }

        hasher.update(b"file:");
        hasher.update(relative.as_bytes());
        hasher.update([0]);

        let mut file = fs::File::open(path)?;
        let mut buffer = [0_u8; 8192];
        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        hasher.update([0]);

        Ok(())
    }

    /// Get user configuration from VM config
    ///
    /// Centralizes the creation of UserConfig to avoid duplication and ensure consistency.
    fn get_user_config(&self) -> UserConfig {
        UserConfig::from_vm_config(self.config)
    }

    fn write_if_changed(path: &Path, content: &str) -> Result<()> {
        match fs::read(path) {
            Ok(existing) if existing == content.as_bytes() => Ok(()),
            _ => fs::write(path, content).map_err(Into::into),
        }
    }
}
