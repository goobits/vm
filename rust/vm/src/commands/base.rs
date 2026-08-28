use crate::cli::BaseSubcommand;
use crate::error::{VmError, VmResult};
use vm_config::{config::VmConfig, AppConfig};
use vm_core::vm_println;
use vm_provider::{build_tart_vibe_base, ensure_configured_tart_vibe_base, TartBaseSource};

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

    match provider {
        "docker" => {
            let build_context = stage_docker_base()?;
            let dockerfile = build_context.path().join("Dockerfile.vibe");
            let config = AppConfig {
                global: Default::default(),
                vm: VmConfig::default(),
            };
            vm_snapshot::handle_create(
                &config,
                "docker",
                DOCKER_BASE_NAME,
                Some("Vibe Docker base"),
                false,
                None,
                Some(&dockerfile),
                Some(build_context.path()),
                &[],
                true,
            )
            .await?;
            vm_println!("Built Docker vibe base: {}", DOCKER_BASE_NAME);
        }
        "tart" => {
            let guest_os = resolve_tart_guest_os(guest_os)?;
            let config = VmConfig::load(None).ok();
            let base_name = build_tart_vibe_base(config.as_ref(), guest_os)?;
            vm_println!("Built Tart {guest_os} vibe base: {base_name}");
        }
        _ => unreachable!(),
    }

    Ok(())
}

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
    use super::{resolve_tart_guest_os, stage_docker_base, DOCKER_BASE_DOCKERFILE};

    #[test]
    fn explicit_tart_guest_os_is_validated() {
        assert_eq!(resolve_tart_guest_os("linux").unwrap(), "linux");
        assert_eq!(resolve_tart_guest_os("macos").unwrap(), "macos");
        assert!(resolve_tart_guest_os("windows").is_err());
    }

    #[test]
    fn docker_base_is_staged_from_the_embedded_definition() {
        let context = stage_docker_base().unwrap();
        let staged = std::fs::read_to_string(context.path().join("Dockerfile.vibe")).unwrap();

        assert_eq!(staged, DOCKER_BASE_DOCKERFILE);
        assert!(staged.contains("FROM "));
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
