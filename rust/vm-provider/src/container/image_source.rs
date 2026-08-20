use std::path::{Path, PathBuf};
use std::process::Command;

use vm_core::command_stream::stream_command_visible;
use vm_core::error::{Result, VmError};
use vm_core::{vm_dbg, vm_info};
use vm_snapshot::{SnapshotManager, SnapshotScope};

use super::{BuildOperations, ContainerOps};
use crate::BoxConfig;

impl<'a> BuildOperations<'a> {
    /// Get box configuration, parsing BoxSpec from vm.box field
    pub(super) fn get_box_config(&self) -> Result<BoxConfig> {
        let base_dir = self.config.project_dir()?;

        if let Some(vm_settings) = &self.config.vm {
            if let Some(box_spec) = vm_settings.get_box_spec() {
                return BoxConfig::parse_for_docker(&box_spec, &base_dir);
            }
        }

        // Default to ubuntu:24.04
        Ok(BoxConfig::DockerImage("ubuntu:24.04".to_string()))
    }

    /// Get the generated custom image name for Dockerfiles
    pub(super) fn get_custom_image_name(&self) -> String {
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
        // Check if image already exists locally to avoid unnecessary pulls (10-30s savings)
        let inspect = Command::new(self.executable)
            .args(["image", "inspect", image])
            .output()?;

        if inspect.status.success() {
            vm_dbg!("Image '{}' already cached locally, skipping pull", image);
            return Ok(());
        }

        // Retry transient network failures with exponential backoff. We keep
        // the attempt count small so a genuinely unreachable registry doesn't
        // stall environment creation for minutes; permanent errors (rate limits, auth,
        // missing manifests) short-circuit immediately.
        const MAX_ATTEMPTS: u32 = 3;
        let mut last_stderr = String::new();
        for attempt in 1..=MAX_ATTEMPTS {
            if attempt == 1 {
                vm_info!("Pulling image '{}'...", image);
            } else {
                vm_info!(
                    "Pulling image '{}' (attempt {}/{})...",
                    image,
                    attempt,
                    MAX_ATTEMPTS
                );
            }

            let output = Command::new(self.executable)
                .args(["pull", image])
                .output()?;

            if output.status.success() {
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

            // Detect rate limiting -- permanent until external state changes.
            if stderr.contains("toomanyrequests") || stderr.contains("rate limit") {
                return Err(VmError::Internal(
                    "Docker Hub rate limit reached\n\n\
                    Fixes:\n\
                      • Wait 6 hours and try again\n\
                      • Login to Docker Hub: docker login"
                        .to_string(),
                ));
            }

            if attempt < MAX_ATTEMPTS && Self::is_transient_pull_error(&stderr) {
                let delay = std::time::Duration::from_secs(1u64 << attempt);
                vm_info!(
                    "Transient pull failure for '{}', retrying in {}s...",
                    image,
                    delay.as_secs()
                );
                std::thread::sleep(delay);
                last_stderr = stderr;
                continue;
            }

            return Err(VmError::Internal(Self::image_pull_error_message(
                image, &stderr,
            )));
        }

        Err(VmError::Internal(Self::image_pull_error_message(
            image,
            &last_stderr,
        )))
    }

    /// Returns true if an image-pull stderr looks like a transient network
    /// failure that's worth retrying rather than reporting straight away.
    fn is_transient_pull_error(stderr: &str) -> bool {
        const TRANSIENT_MARKERS: &[&str] = &[
            "connection reset",
            "connection refused",
            "i/o timeout",
            "TLS handshake timeout",
            "tls: bad record MAC",
            "EOF",
            "context deadline exceeded",
            "temporary failure in name resolution",
            "no route to host",
            "network is unreachable",
            "broken pipe",
        ];
        TRANSIENT_MARKERS
            .iter()
            .any(|marker| stderr.contains(marker))
    }

    pub(super) fn image_pull_error_message(image: &str, stderr: &str) -> String {
        if stderr.contains("unshare: operation not permitted")
            || stderr.contains("failed to register layer")
                && stderr.contains("operation not permitted")
        {
            return format!(
                "Image pull failed for '{image}': the container engine cannot register image layers in this environment.\n\n\
                 This usually means vm is running inside an unprivileged container where Linux namespace or mount operations are blocked.\n\n\
                 Fixes:\n\
                   • Run vm from the host machine, not inside this container\n\
                   • If a nested container engine is intentional, start the outer container with the required privileges\n\
                   • On macOS, run vm from your normal terminal with Docker Desktop, Podman, or Tart available\n\n\
                 Raw engine error: {stderr}"
            );
        }

        format!("Image pull failed for '{image}': {stderr}")
    }

    /// Safely convert a path to string with descriptive error message
    pub(crate) fn path_to_string(path: &Path) -> Result<&str> {
        path.to_str().ok_or_else(|| {
            VmError::Internal(format!(
                "Path '{}' contains invalid UTF-8 characters and cannot be used as a container build argument",
                path.display()
            ))
        })
    }

    /// Prepare build context with embedded resources and generated Dockerfile
    ///
    /// Returns a tuple of (build_context_path, base_image_name, is_snapshot)
    pub fn prepare_build_context(&self) -> Result<(PathBuf, String, bool)> {
        use vm_core::vm_info;

        // Get box configuration
        let box_config = self.get_box_config()?;

        // Track if we're using a pre-provisioned snapshot
        let is_snapshot = matches!(&box_config, BoxConfig::Snapshot(_));

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
                ContainerOps::build_custom_image(
                    Some(self.executable),
                    path,
                    &image_name,
                    context,
                    args.as_ref(),
                )?;

                image_name
            }
            BoxConfig::Snapshot(name) => {
                // Load image from global snapshot
                use vm_core::vm_println;

                vm_println!("Loading base image from snapshot '@{}'...", name);

                let manager = SnapshotManager::new()?;
                let snapshot_dir = manager.get_snapshot_dir(SnapshotScope::Global, name)?;

                if !snapshot_dir.exists() {
                    return Err(VmError::Config(format!(
                        "Snapshot '@{}' not found. Create or import it first:\n  vm package --build <dockerfile>\n  vm package <name>",
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
                let image_exists = match Command::new(self.executable)
                    .args(["image", "inspect", image_tag])
                    .output()
                {
                    Ok(output) => output.status.success(),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        return Err(VmError::Dependency(format!(
                            "Container engine '{}' is not installed or not in PATH",
                            self.executable
                        )));
                    }
                    Err(e) => {
                        return Err(VmError::Internal(format!(
                            "Failed to inspect the container image with '{}': {}",
                            self.executable, e
                        )));
                    }
                };

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

                    // Stream output so user sees docker load progress
                    stream_command_visible(
                        self.executable,
                        &["load", "-i", Self::path_to_string(&image_file_path)?],
                    )
                    .map_err(|e| {
                        VmError::Internal(format!(
                            "Failed to load container image from snapshot: {}",
                            e
                        ))
                    })?;

                    vm_println!("  ✓ Image loaded successfully");
                }

                image_tag.to_string()
            }
            _ => {
                return Err(VmError::Internal(
                    "Invalid box configuration for container provider".to_string(),
                ));
            }
        };

        let build_context = self.prepare_compose_build_context()?;

        Ok((build_context, base_image, is_snapshot))
    }
}
