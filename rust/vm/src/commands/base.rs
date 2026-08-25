use crate::cli::BaseSubcommand;
use crate::error::{VmError, VmResult};
use std::process::Command;
use vm_config::{
    config::{ImageSpec, VmConfig},
    resolve_tool_path, AppConfig,
};
use vm_core::{vm_println, vm_warning};
use vm_provider::{
    versioned_tart_cache_name, versioned_tart_image, TartCommand, TART_LINUX_NAME, TART_MACOS_NAME,
};

mod runtime;
mod tart_install;

pub(in crate::commands) use runtime::{
    codex_expected, codex_state, reconcile_codex, reconcile_codex_in_background, CodexState,
};

const DOCKER_BASE_NAME: &str = "@vibe-image";
const TART_BASE_BUILDER: &str = include_str!("../../scripts/build-vibe-tart-base.sh");
const VIBE_AI_TOOLS_INSTALLER: &str = include_str!("../../scripts/install-vibe-ai-tools.sh");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TartVibeBase {
    guest_os: &'static str,
}

impl TartVibeBase {
    fn local_name(self) -> String {
        tart_base_local_name(self.guest_os)
    }

    fn prebuilt_image(self) -> Option<String> {
        (self.guest_os == "linux").then(versioned_tart_image)
    }
}

pub async fn handle_base(command: BaseSubcommand) -> VmResult<()> {
    match command {
        BaseSubcommand::Build {
            preset,
            provider,
            guest_os,
        } => handle_build(&preset, &provider, &guest_os).await,
        BaseSubcommand::Validate {
            preset,
            provider,
            rebuild_docker_base,
            build_tart_base,
        } => handle_validate(&preset, &provider, rebuild_docker_base, build_tart_base),
    }
}

async fn handle_build(preset: &str, provider: &str, guest_os: &str) -> VmResult<()> {
    ensure_supported_preset(preset)?;

    match provider {
        "docker" => {
            let dockerfile = resolve_tool_path("Dockerfile.vibe");
            let build_context = dockerfile.parent().ok_or_else(|| {
                VmError::validation(
                    "Vibe Dockerfile has no build context",
                    Some(dockerfile.display().to_string()),
                )
            })?;
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
                Some(build_context),
                &[],
                true,
            )
            .await?;
            vm_println!("Built Docker vibe base: {}", DOCKER_BASE_NAME);
        }
        "tart" => {
            let guest_os = resolve_tart_guest_os(guest_os)?;
            let base_name = tart_base_local_name(guest_os);
            let config = VmConfig::load(None).ok();
            let command = TartCommand::from_config(config.as_ref());
            tart_install::build(&command, guest_os, &base_name, TART_BASE_BUILDER)?;
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

fn tart_base_name(guest_os: &str) -> &'static str {
    if guest_os == "macos" {
        TART_MACOS_NAME
    } else {
        TART_LINUX_NAME
    }
}

fn tart_base_local_name(guest_os: &str) -> String {
    if guest_os == "linux" {
        versioned_tart_cache_name()
    } else {
        tart_base_name(guest_os).to_string()
    }
}

fn configured_tart_vibe_base(config: &VmConfig) -> Option<TartVibeBase> {
    if config.provider.as_deref() != Some("tart") {
        return None;
    }

    let ImageSpec::String(name) = config.vm.as_ref()?.image.clone()? else {
        return None;
    };

    match name.as_str() {
        TART_LINUX_NAME => Some(TartVibeBase { guest_os: "linux" }),
        TART_MACOS_NAME => Some(TartVibeBase { guest_os: "macos" }),
        _ => None,
    }
}

pub(super) fn ensure_configured_tart_base(config: &VmConfig) -> VmResult<()> {
    let Some(base) = configured_tart_vibe_base(config) else {
        return Ok(());
    };

    ensure_tart_vibe_base(base, Some(config)).map(|_| ())
}

fn ensure_tart_vibe_base(base: TartVibeBase, config: Option<&VmConfig>) -> VmResult<String> {
    let command = TartCommand::from_config(config);
    ensure_tart_vibe_base_with_command(base, &command)
}

fn ensure_tart_vibe_base_with_command(
    base: TartVibeBase,
    command: &TartCommand,
) -> VmResult<String> {
    let local_name = base.local_name();

    if tart_install::exists(command, &local_name)?
        && tart_install::receipt_matches(command, &local_name, base.guest_os)?
    {
        return Ok(local_name);
    }

    if let Some(image) = base.prebuilt_image() {
        vm_println!(
            "Tart vibe base '{}' is missing; pulling '{}'...",
            local_name,
            image
        );
        if tart_install::pull(command, &image, &local_name, base.guest_os)? {
            vm_println!("Pulled Tart Linux vibe base: {}", local_name);
            return Ok(local_name);
        }
        vm_warning!(
            "Prebuilt Tart base '{}' is unavailable; building the Linux base locally instead.",
            image
        );
    }

    vm_println!(
        "Tart vibe base '{}' is missing; building it now...",
        local_name
    );
    tart_install::build(command, base.guest_os, &local_name, TART_BASE_BUILDER).map_err(
        |error| {
            VmError::validation(
                format!("Could not prepare Tart vibe base '{local_name}': {error}"),
                Some("Run `vm system base build vibe --provider tart` to retry with full output"),
            )
        },
    )?;
    Ok(local_name)
}

fn apply_tart_home_from_config(command: &mut Command) {
    let config = VmConfig::load(None).ok();
    TartCommand::from_config(config.as_ref()).configure(command);
}

fn handle_validate(
    preset: &str,
    provider: &str,
    rebuild_docker_base: bool,
    build_tart_base: bool,
) -> VmResult<()> {
    ensure_supported_preset(preset)?;

    let script = resolve_tool_path("scripts/internal/validate-vibe-providers.sh");
    let mut cmd = Command::new(script);
    apply_tart_home_from_config(&mut cmd);

    match provider {
        "docker" => {
            if rebuild_docker_base {
                cmd.arg("--rebuild-docker-base");
            }
            cmd.args(["--provider", "docker"]);
        }
        "tart" => {
            if build_tart_base {
                cmd.arg("--build-tart-base");
            }
            cmd.args(["--provider", "tart"]);
        }
        "all" => {
            if rebuild_docker_base {
                cmd.arg("--rebuild-docker-base");
            }
            if build_tart_base {
                cmd.arg("--build-tart-base");
            }
            cmd.args(["--provider", "all"]);
        }
        _ => unreachable!(),
    }

    run_command(cmd, "validate vibe providers")
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

fn run_command(mut command: Command, context: &str) -> VmResult<()> {
    let status = command.status().map_err(|error| {
        let message = format!("Failed to {context}: {error}");
        VmError::general(error, message)
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(VmError::validation(
            format!("{context} failed with {status}"),
            None::<String>,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        configured_tart_vibe_base, resolve_tart_guest_os, tart_base_local_name, tart_base_name,
        TartVibeBase, TART_BASE_BUILDER, VIBE_AI_TOOLS_INSTALLER,
    };
    use std::ffi::OsStr;
    use vm_config::config::{ImageSpec, TartConfig, VmConfig, VmSettings};
    use vm_provider::{
        versioned_tart_cache_name, versioned_tart_image, TartCommand, TART_LINUX_NAME,
        TART_MACOS_NAME,
    };

    fn config(provider: &str, image_name: &str) -> VmConfig {
        VmConfig {
            provider: Some(provider.into()),
            vm: Some(VmSettings {
                image: Some(ImageSpec::String(image_name.to_string())),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn explicit_tart_guest_os_resolves_base_name() {
        assert_eq!(resolve_tart_guest_os("linux").unwrap(), "linux");
        assert_eq!(resolve_tart_guest_os("macos").unwrap(), "macos");
        assert_eq!(tart_base_name("linux"), "vibe-tart-linux-base");
        assert_eq!(tart_base_name("macos"), "vibe-tart-sequoia-base");
        assert_eq!(tart_base_local_name("linux"), versioned_tart_cache_name());
        assert_eq!(tart_base_local_name("macos"), "vibe-tart-sequoia-base");
    }

    #[test]
    fn tart_base_builder_is_embedded_in_the_binary() {
        assert!(TART_BASE_BUILDER.starts_with("#!/usr/bin/env bash"));
        assert!(TART_BASE_BUILDER.contains("tart clone"));
        assert!(TART_BASE_BUILDER.contains("--guest-os"));
        assert!(!TART_BASE_BUILDER.contains("tart delete \"$BASE_NAME\""));
    }

    #[test]
    fn vibe_bases_own_standard_ai_clis() {
        const DOCKERFILE: &str = include_str!("../../../../Dockerfile.vibe");
        const VIBE_PRESET: &str = include_str!("../../../../plugins/vibe-dev/preset.yaml");

        for installer in [
            "https://antigravity.google/cli/install.sh",
            "https://claude.ai/install.sh",
            "https://chatgpt.com/codex/install.sh",
        ] {
            assert!(VIBE_AI_TOOLS_INSTALLER.contains(installer));
            assert!(DOCKERFILE.contains(installer));
        }
        assert!(TART_BASE_BUILDER.contains("VIBE_AI_TOOLS_INSTALLER"));
        assert!(TART_BASE_BUILDER.contains("antigravity claude codex"));
        for runtime_contract in [
            "codex-package.json",
            "cp -R \"$codex_package_dir/.\"",
            "/usr/local/lib/vm-ai-tools/codex-package/bin/codex",
            "/usr/local/lib/vm-ai-tools/codex-package/bin/codex-code-mode-host",
        ] {
            assert!(VIBE_AI_TOOLS_INSTALLER.contains(runtime_contract));
            assert!(DOCKERFILE.contains(runtime_contract));
        }
        assert!(VIBE_PRESET.contains("agent-skills: {}"));
        for managed_entry in ["antigravity: {}", "claude: {}", "codex: {}"] {
            assert!(!VIBE_PRESET.contains(managed_entry));
        }
        assert!(DOCKERFILE.contains("CARGO_TARGET_DIR=\"/tmp/vm-rust-target\""));
        assert!(DOCKERFILE.contains("CMD command -v node >/dev/null && test -x /usr/bin/python3"));
        assert!(!DOCKERFILE.contains("CMD bash -c 'source ~/.nvm/nvm.sh"));
    }

    #[test]
    fn invalid_tart_guest_os_is_rejected() {
        assert!(resolve_tart_guest_os("windows").is_err());
    }

    #[test]
    fn configured_vibe_bases_resolve_their_guest_os() {
        assert_eq!(
            configured_tart_vibe_base(&config("tart", TART_MACOS_NAME)),
            Some(TartVibeBase { guest_os: "macos" })
        );
        assert_eq!(
            configured_tart_vibe_base(&config("tart", TART_LINUX_NAME)),
            Some(TartVibeBase { guest_os: "linux" })
        );
        assert_eq!(
            configured_tart_vibe_base(&config("tart", TART_LINUX_NAME))
                .and_then(TartVibeBase::prebuilt_image),
            Some(versioned_tart_image())
        );
        assert_eq!(
            configured_tart_vibe_base(&config("tart", TART_LINUX_NAME))
                .map(TartVibeBase::local_name),
            Some(versioned_tart_cache_name())
        );
        assert_eq!(
            configured_tart_vibe_base(&config("tart", TART_MACOS_NAME))
                .and_then(TartVibeBase::prebuilt_image),
            None
        );
    }

    #[test]
    fn custom_images_and_other_providers_do_not_auto_build() {
        assert_eq!(
            configured_tart_vibe_base(&config("tart", "ghcr.io/example/custom:latest")),
            None
        );
        assert_eq!(
            configured_tart_vibe_base(&config("docker", TART_MACOS_NAME)),
            None
        );
    }

    #[test]
    fn tart_storage_path_is_forwarded_to_commands() {
        let config = VmConfig {
            tart: Some(TartConfig {
                storage_path: Some("/Volumes/ExternalSSD/Tart".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let command = TartCommand::from_config(Some(&config)).command();

        assert!(command.get_envs().any(|(key, value)| key == "TART_HOME"
            && value == Some(OsStr::new("/Volumes/ExternalSSD/Tart"))));
    }
}
