use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::info;

use vm_config::config::ImageSpec;
use vm_core::command_stream::stream_command;
use vm_core::error::{Result, VmError};
use vm_core::vm_dbg;
use vm_snapshot::{SnapshotManager, SnapshotScope};

use super::{BuildOperations, ContainerOps};
use crate::tart_base;

#[derive(Debug, Clone)]
pub(super) enum ContainerImageSource {
    Image(String),
    Dockerfile {
        path: PathBuf,
        context: PathBuf,
        args: Option<HashMap<String, String>>,
    },
    Snapshot(String),
}

impl ContainerImageSource {
    fn parse(spec: &ImageSpec, base_dir: &Path) -> Result<Self> {
        match spec {
            ImageSpec::String(value) => {
                if let Some(name) = value.strip_prefix('@') {
                    return Ok(Self::Snapshot(name.to_string()));
                }

                let candidate = Path::new(value);
                if value.starts_with("./") || value.starts_with("../") || candidate.is_absolute() {
                    let path = if candidate.is_absolute() {
                        candidate.to_path_buf()
                    } else {
                        base_dir.join(candidate)
                    };
                    let context = path.parent().unwrap_or(base_dir).to_path_buf();
                    return Ok(Self::Dockerfile {
                        path,
                        context,
                        args: None,
                    });
                }
                if value.ends_with(".dockerfile") {
                    return Ok(Self::Dockerfile {
                        path: base_dir.join(value),
                        context: base_dir.to_path_buf(),
                        args: None,
                    });
                }
                let lower = value.to_ascii_lowercase();
                if tart_base::guest_os(value).is_some()
                    || value.starts_with(tart_base::LINUX_REGISTRY)
                    || lower.contains("cirruslabs/macos")
                {
                    return Err(VmError::Config(format!(
                        "'{value}' looks like a Tart image, but the Docker provider was selected. Use provider: tart or choose a Docker image/Dockerfile."
                    )));
                }
                Ok(Self::Image(value.clone()))
            }
            ImageSpec::Build {
                dockerfile,
                context,
                args,
            } => {
                let dockerfile = Path::new(dockerfile);
                let path = if dockerfile.is_absolute() {
                    dockerfile.to_path_buf()
                } else {
                    base_dir.join(dockerfile)
                };
                let context = context.as_deref().map_or_else(
                    || path.parent().unwrap_or(base_dir).to_path_buf(),
                    |context| {
                        let context = Path::new(context);
                        if context.is_absolute() {
                            context.to_path_buf()
                        } else {
                            base_dir.join(context)
                        }
                    },
                );
                Ok(Self::Dockerfile {
                    path,
                    context,
                    args: args.clone().map(|args| args.into_iter().collect()),
                })
            }
        }
    }
}

impl<'a> BuildOperations<'a> {
    /// Get image configuration, parsing ImageSpec from vm.image field
    pub(super) fn get_image_config(&self) -> Result<ContainerImageSource> {
        let base_dir = self.config.project_dir()?;

        if let Some(vm_settings) = &self.config.vm {
            if let Some(image_spec) = vm_settings.image.clone() {
                return ContainerImageSource::parse(&image_spec, &base_dir);
            }
        }

        // Default to ubuntu:24.04
        Ok(ContainerImageSource::Image("ubuntu:24.04".to_string()))
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

    fn pull_image_with_identity(&self, image: &str) -> Result<String> {
        match self.ensure_image_available(image)? {
            Some(identity) => Self::parse_image_identity(image, &identity),
            None => self.image_identity(image),
        }
    }

    fn ensure_image_available(&self, image: &str) -> Result<Option<Vec<u8>>> {
        // Check if image already exists locally to avoid unnecessary pulls (10-30s savings)
        let inspect = Command::new(self.executable)
            .args(["image", "inspect", "--format", "{{.Id}}", image])
            .output()?;

        if inspect.status.success() {
            vm_dbg!("Image '{}' already cached locally, skipping pull", image);
            return Ok(Some(inspect.stdout));
        }

        // Retry transient network failures with exponential backoff. We keep
        // the attempt count small so a genuinely unreachable registry doesn't
        // stall environment creation for minutes; permanent errors (rate limits, auth,
        // missing manifests) short-circuit immediately.
        const MAX_ATTEMPTS: u32 = 3;
        let mut last_stderr = String::new();
        for attempt in 1..=MAX_ATTEMPTS {
            if attempt == 1 {
                info!("Pulling image '{}'...", image);
            } else {
                info!(
                    "Pulling image '{}' (attempt {}/{})...",
                    image, attempt, MAX_ATTEMPTS
                );
            }

            let output = Command::new(self.executable)
                .args(["pull", image])
                .output()?;

            if output.status.success() {
                return Ok(None);
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
                info!(
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
    /// Returns the build context, base image, snapshot flag, and any image ID
    /// already observed while ensuring that the base image is available.
    pub(crate) fn prepare_build_context(&self) -> Result<(PathBuf, String, bool, Option<String>)> {
        // Get image configuration
        let image_config = self.get_image_config()?;

        // Track if we're using a pre-provisioned snapshot
        let is_snapshot = matches!(&image_config, ContainerImageSource::Snapshot(_));

        // Handle different image types
        let (base_image, base_image_identity) = match &image_config {
            ContainerImageSource::Image(image) => {
                // Pull Docker image from registry
                let identity = self.pull_image_with_identity(image)?;
                (image.clone(), Some(identity))
            }
            ContainerImageSource::Dockerfile {
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

                info!("Building from custom Dockerfile: {}", path.display());

                // Build the image with a generated name
                let image_name = self.get_custom_image_name();

                // Pass build args from ImageSpec::Build variant
                ContainerOps::build_custom_image(
                    Some(self.executable),
                    path,
                    &image_name,
                    context,
                    args.as_ref(),
                )?;

                (image_name, None)
            }
            ContainerImageSource::Snapshot(name) => {
                // Load image from global snapshot
                info!("Loading base image from snapshot '@{}'...", name);

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
                let image_identity = match Command::new(self.executable)
                    .args(["image", "inspect", "--format", "{{.Id}}", image_tag])
                    .output()
                {
                    Ok(output) if output.status.success() => {
                        Some(Self::parse_image_identity(image_tag, &output.stdout)?)
                    }
                    Ok(_) => None,
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

                if image_identity.is_none() {
                    info!("  Image not loaded, loading from snapshot...");

                    // Load image from tar file
                    let image_file_path = snapshot_dir.join("images").join("base.tar");

                    if !image_file_path.exists() {
                        return Err(VmError::Config(format!(
                            "Snapshot '@{}' is corrupted (base.tar not found)",
                            name
                        )));
                    }

                    stream_command(
                        self.executable,
                        &["load", "-i", Self::path_to_string(&image_file_path)?],
                    )
                    .map_err(|e| {
                        VmError::Internal(format!(
                            "Failed to load container image from snapshot: {}",
                            e
                        ))
                    })?;

                    info!("  ✓ Image loaded successfully");
                }

                (image_tag.to_string(), image_identity)
            }
        };

        let build_context = self.prepare_compose_build_context()?;

        Ok((build_context, base_image, is_snapshot, base_image_identity))
    }
}

#[cfg(test)]
mod tests {
    use super::ContainerImageSource;
    use indexmap::IndexMap;
    use std::path::{Path, PathBuf};
    use vm_config::config::ImageSpec;

    #[test]
    fn parses_registry_images_snapshots_and_dockerfile_paths() {
        let base = Path::new("/workspace");
        assert!(matches!(
            ContainerImageSource::parse(&ImageSpec::String("ubuntu:24.04".into()), base).unwrap(),
            ContainerImageSource::Image(image) if image == "ubuntu:24.04"
        ));
        assert!(matches!(
            ContainerImageSource::parse(&ImageSpec::String("@release".into()), base).unwrap(),
            ContainerImageSource::Snapshot(name) if name == "release"
        ));

        for value in ["./Dockerfile", "../Dockerfile", "build/app.dockerfile"] {
            assert!(matches!(
                ContainerImageSource::parse(&ImageSpec::String(value.into()), base).unwrap(),
                ContainerImageSource::Dockerfile { .. }
            ));
        }
    }

    #[test]
    fn resolves_build_paths_context_and_arguments() {
        let mut args = IndexMap::new();
        args.insert("NODE_VERSION".into(), "20".into());
        let source = ContainerImageSource::parse(
            &ImageSpec::Build {
                dockerfile: "docker/Dockerfile".into(),
                context: Some("docker".into()),
                args: Some(args),
            },
            Path::new("/workspace"),
        )
        .unwrap();

        let ContainerImageSource::Dockerfile {
            path,
            context,
            args,
        } = source
        else {
            panic!("expected Dockerfile source");
        };
        assert_eq!(path, PathBuf::from("/workspace/docker/Dockerfile"));
        assert_eq!(context, PathBuf::from("/workspace/docker"));
        assert_eq!(
            args.unwrap().get("NODE_VERSION").map(String::as_str),
            Some("20")
        );
    }

    #[test]
    fn preserves_absolute_build_paths() {
        let source = ContainerImageSource::parse(
            &ImageSpec::Build {
                dockerfile: "/src/Dockerfile".into(),
                context: Some("/src".into()),
                args: None,
            },
            Path::new("/workspace"),
        )
        .unwrap();
        assert!(matches!(
            source,
            ContainerImageSource::Dockerfile { path, context, .. }
                if path == Path::new("/src/Dockerfile") && context == Path::new("/src")
        ));
    }

    #[test]
    fn rejects_known_tart_images() {
        for value in [
            "vibe-tart-sequoia-base",
            "ghcr.io/cirruslabs/macos-sequoia-base:latest",
        ] {
            let error = ContainerImageSource::parse(
                &ImageSpec::String(value.into()),
                Path::new("/workspace"),
            )
            .unwrap_err();
            assert!(error.to_string().contains("looks like a Tart image"));
        }
    }

    #[test]
    fn image_names_are_not_confused_with_paths() {
        assert!(matches!(
            ContainerImageSource::parse(
                &ImageSpec::String("registry.example/path/image:tag".into()),
                Path::new("/workspace"),
            )
            .unwrap(),
            ContainerImageSource::Image(_)
        ));
        assert!(matches!(
            ContainerImageSource::parse(
                &ImageSpec::String("app.Dockerfile".into()),
                Path::new("/workspace"),
            )
            .unwrap(),
            ContainerImageSource::Image(_)
        ));
    }
}
