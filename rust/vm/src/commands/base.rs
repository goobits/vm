use crate::cli::BaseSubcommand;
use crate::error::{VmError, VmResult};
use std::process::Command;
use vm_config::{config::VmConfig, AppConfig};
use vm_core::{vm_println, vm_progress};
#[cfg(any(target_os = "macos", feature = "tart"))]
use vm_provider::{build_tart_vibe_base, ensure_configured_tart_vibe_base, TartBaseSource};
use vm_snapshot::{SnapshotManager, SnapshotMetadata, SnapshotScope};

mod runtime;

pub(in crate::commands) use runtime::{
    is_vendor_tool, reconcile_vendor_tools, update_vendor_tools, vendor_tool_info,
    vendor_tool_statuses, vendor_tools_expected, VendorToolState,
};

const DOCKER_BASE_NAME: &str = "@vibe-image";
const DOCKER_BASE_DOCKERFILE: &str = include_str!("../../../../Dockerfile.vibe");

pub async fn handle_base(command: BaseSubcommand) -> VmResult<()> {
    match command {
        BaseSubcommand::Build {
            preset,
            provider,
            guest_os,
        } => handle_build(&preset, &provider, &guest_os).await,
    }
}

fn stage_docker_base() -> VmResult<tempfile::TempDir> {
    let context = tempfile::tempdir()
        .map_err(|error| VmError::general(error, "Failed to create a Docker base build context"))?;
    std::fs::write(
        context.path().join("Dockerfile.vibe"),
        DOCKER_BASE_DOCKERFILE,
    )
    .map_err(|error| VmError::general(error, "Failed to stage the Docker base definition"))?;
    Ok(context)
}

async fn handle_build(preset: &str, provider: &str, guest_os: &str) -> VmResult<()> {
    ensure_supported_preset(preset)?;
    preflight_build(provider, guest_os)?;

    match provider {
        "docker" | "podman" => {
            build_container_base(provider).await?;
            vm_println!("Built {provider} vibe base: {}", DOCKER_BASE_NAME);
        }
        "tart" => build_tart_base(guest_os)?,
        _ => unreachable!(),
    }

    Ok(())
}

fn preflight_build(provider: &str, guest_os: &str) -> VmResult<()> {
    if provider != "tart" && guest_os != "auto" {
        return Err(VmError::validation(
            "--guest-os applies only to the Tart provider",
            None::<String>,
        ));
    }
    let (executable, args): (&str, &[&str]) = match provider {
        "docker" | "podman" => (provider, &["info"]),
        "tart" => ("tart", &["--version"]),
        _ => {
            return Err(VmError::validation(
                format!("Unsupported base image provider '{provider}'"),
                None::<String>,
            ))
        }
    };
    let output = Command::new(executable)
        .args(args)
        .output()
        .map_err(|error| {
            VmError::validation(
                format!("{provider} is unavailable for base image builds: {error}"),
                Some(format!("Install and start {provider}, then retry")),
            )
        })?;
    if !output.status.success() {
        return Err(VmError::validation(
            format!("{provider} is not ready for base image builds"),
            Some(String::from_utf8_lossy(&output.stderr).trim().to_string()),
        ));
    }
    Ok(())
}

async fn build_container_base(executable: &str) -> VmResult<()> {
    let build_context = stage_docker_base()?;
    let dockerfile = build_context.path().join("Dockerfile.vibe");
    let config = AppConfig {
        global: Default::default(),
        vm: VmConfig::default(),
    };
    vm_snapshot::handle_create(
        &config,
        executable,
        DOCKER_BASE_NAME,
        Some("Vibe Docker base"),
        false,
        None,
        None,
        None,
        Some(&dockerfile),
        Some(build_context.path()),
        &[],
        true,
    )
    .await
    .map_err(VmError::from)
}

pub(super) async fn ensure_configured_container_base(
    config: &VmConfig,
    executable: &str,
) -> VmResult<()> {
    if !uses_docker_vibe_base(config) {
        return Ok(());
    }

    let manager = SnapshotManager::new()?;
    if manager.snapshot_exists(SnapshotScope::Global, "vibe-image")? {
        return Ok(());
    }

    vm_progress!("Preparing the Vibe base image for first use (this may take several minutes)...");
    build_container_base(executable).await?;
    vm_println!("Built {executable} vibe base: {}", DOCKER_BASE_NAME);
    Ok(())
}

/// Catch missing or damaged snapshot files before a forced recreation removes
/// an existing environment. The provider still owns loading the image itself.
pub(super) fn preflight_configured_container_snapshot(config: &VmConfig) -> VmResult<()> {
    use vm_config::config::ImageSpec;

    let Some(ImageSpec::String(image)) = config.vm.as_ref().and_then(|vm| vm.image.as_ref()) else {
        return Ok(());
    };
    let Some(name) = image.strip_prefix('@') else {
        return Ok(());
    };
    let manager = SnapshotManager::new()?;
    let snapshot_dir = manager.get_snapshot_dir(SnapshotScope::Global, name)?;
    preflight_snapshot_files(name, &snapshot_dir)
}

fn preflight_snapshot_files(name: &str, snapshot_dir: &std::path::Path) -> VmResult<()> {
    let metadata_path = snapshot_dir.join("metadata.json");
    if !metadata_path.is_file() {
        return Err(VmError::validation(
            format!("Snapshot '@{name}' is missing metadata.json"),
            Some("Create or import the snapshot before recreating the environment"),
        ));
    }
    let metadata = SnapshotMetadata::load(&metadata_path)?;
    if !metadata
        .services
        .first()
        .is_some_and(|service| !service.image_tag.trim().is_empty())
    {
        return Err(VmError::validation(
            format!("Snapshot '@{name}' has no base image tag"),
            Some("Rebuild or re-import the snapshot before recreating the environment"),
        ));
    }
    let archive = snapshot_dir.join("images/base.tar");
    if !archive.is_file() || std::fs::metadata(&archive)?.len() == 0 {
        return Err(VmError::validation(
            format!("Snapshot '@{name}' has no usable images/base.tar"),
            Some("Rebuild or re-import the snapshot before recreating the environment"),
        ));
    }
    Ok(())
}

fn uses_docker_vibe_base(config: &VmConfig) -> bool {
    use vm_config::config::ImageSpec;

    matches!(
        config.vm.as_ref().and_then(|settings| settings.image.as_ref()),
        Some(ImageSpec::String(image)) if image == DOCKER_BASE_NAME
    )
}

#[cfg(any(target_os = "macos", feature = "tart"))]
fn build_tart_base(requested_guest_os: &str) -> VmResult<()> {
    let guest_os = resolve_tart_guest_os(requested_guest_os)?;
    let config = VmConfig::load(None).ok();
    let base_name = build_tart_vibe_base(config.as_ref(), guest_os)?;
    vm_println!("Built Tart {guest_os} vibe base: {base_name}");
    Ok(())
}

#[cfg(not(any(target_os = "macos", feature = "tart")))]
fn build_tart_base(_requested_guest_os: &str) -> VmResult<()> {
    Err(VmError::validation(
        "Tart provider support is not enabled in this build",
        None::<String>,
    ))
}

#[cfg(any(target_os = "macos", feature = "tart", test))]
fn resolve_tart_guest_os(requested: &str) -> VmResult<&'static str> {
    match requested {
        "linux" => Ok("linux"),
        "macos" => Ok("macos"),
        "auto" => Ok(active_tart_guest_os()),
        _ => Err(VmError::validation(
            "Invalid Tart guest OS",
            Some("Use linux, macos, or auto"),
        )),
    }
}

#[cfg(any(target_os = "macos", feature = "tart", test))]
fn active_tart_guest_os() -> &'static str {
    let Ok(app_config) = AppConfig::load(None, None, Some("tart".to_string())) else {
        return "linux";
    };

    if app_config
        .vm
        .tart
        .as_ref()
        .and_then(|tart| tart.guest_os.as_deref())
        == Some("macos")
    {
        "macos"
    } else {
        "linux"
    }
}

#[cfg(any(target_os = "macos", feature = "tart"))]
pub(super) fn ensure_configured_tart_base(config: &VmConfig) -> VmResult<()> {
    let Some(prepared) = ensure_configured_tart_vibe_base(config)? else {
        return Ok(());
    };
    match prepared.source {
        TartBaseSource::Current => {}
        TartBaseSource::Pulled => {
            vm_println!(
                "Pulled Tart {} vibe base: {}",
                prepared.guest_os,
                prepared.name
            )
        }
        TartBaseSource::Built => {
            vm_println!(
                "Built Tart {} vibe base: {}",
                prepared.guest_os,
                prepared.name
            )
        }
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", feature = "tart")))]
pub(super) fn ensure_configured_tart_base(_config: &VmConfig) -> VmResult<()> {
    Ok(())
}

fn ensure_supported_preset(preset: &str) -> VmResult<()> {
    if preset == "vibe" {
        Ok(())
    } else {
        Err(VmError::validation(
            "Only the 'vibe' base workflow is currently supported",
            None::<String>,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        preflight_build, preflight_snapshot_files, resolve_tart_guest_os, stage_docker_base,
        uses_docker_vibe_base, DOCKER_BASE_DOCKERFILE,
    };

    #[test]
    fn damaged_snapshot_is_rejected_before_recreation() {
        let snapshot = tempfile::tempdir().unwrap();
        let missing = preflight_snapshot_files("vibe-image", snapshot.path()).unwrap_err();
        assert!(missing.to_string().contains("metadata.json"));

        std::fs::write(snapshot.path().join("metadata.json"), "broken json").unwrap();
        assert!(preflight_snapshot_files("vibe-image", snapshot.path()).is_err());
    }

    #[test]
    fn explicit_tart_guest_os_is_validated() {
        assert_eq!(resolve_tart_guest_os("linux").unwrap(), "linux");
        assert_eq!(resolve_tart_guest_os("macos").unwrap(), "macos");
        assert!(resolve_tart_guest_os("windows").is_err());
    }

    #[test]
    fn base_build_rejects_provider_option_mismatch_before_any_runtime_call() {
        assert!(preflight_build("docker", "macos").is_err());
        assert!(preflight_build("unknown", "auto").is_err());
    }

    #[test]
    fn docker_base_is_staged_from_the_embedded_definition() {
        let context = stage_docker_base().unwrap();
        let staged = std::fs::read_to_string(context.path().join("Dockerfile.vibe")).unwrap();

        assert_eq!(staged, DOCKER_BASE_DOCKERFILE);
        assert!(staged.contains("FROM "));
    }

    #[test]
    fn only_the_standard_vibe_snapshot_is_bootstrapped() {
        let vibe = serde_yaml_ng::from_str("vm:\n  image: '@vibe-image'\n").unwrap();
        let custom = serde_yaml_ng::from_str("vm:\n  image: '@team-image'\n").unwrap();
        let registry = serde_yaml_ng::from_str("vm:\n  image: 'ubuntu:24.04'\n").unwrap();

        assert!(uses_docker_vibe_base(&vibe));
        assert!(!uses_docker_vibe_base(&custom));
        assert!(!uses_docker_vibe_base(&registry));
    }

    #[test]
    fn vibe_bases_own_standard_ai_clis() {
        const VIBE_PRESET: &str = include_str!("../../../../plugins/vibe-dev/preset.yaml");

        for installer in [
            "https://antigravity.google/cli/install.sh",
            "https://claude.ai/install.sh",
            "https://chatgpt.com/codex/install.sh",
        ] {
            assert!(DOCKER_BASE_DOCKERFILE.contains(installer));
        }
        for runtime_contract in [
            "codex-package.json",
            "cp -R \"$codex_package_dir/.\"",
            "/usr/local/lib/vm-ai-tools/codex-package/bin/codex",
            "/usr/local/lib/vm-ai-tools/codex-package/bin/codex-code-mode-host",
        ] {
            assert!(DOCKER_BASE_DOCKERFILE.contains(runtime_contract));
        }
        assert!(VIBE_PRESET.contains("agent-skills: {}"));
        for managed_entry in ["antigravity: {}", "claude: {}", "codex: {}"] {
            assert!(!VIBE_PRESET.contains(managed_entry));
        }
        assert!(DOCKER_BASE_DOCKERFILE.contains("CARGO_TARGET_DIR=\"/tmp/vm-rust-target\""));
        assert!(DOCKER_BASE_DOCKERFILE
            .contains("CMD command -v node >/dev/null && test -x /usr/bin/python3"));
        assert!(!DOCKER_BASE_DOCKERFILE.contains("CMD bash -c 'source ~/.nvm/nvm.sh"));
    }
}
