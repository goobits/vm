use crate::cli::BaseSubcommand;
use crate::error::{VmError, VmResult};
use serde::Deserialize;
use std::process::Command;
use vm_config::{
    config::{BoxSpec, VmConfig},
    resolve_tool_path, AppConfig,
};
use vm_core::vm_println;

const DOCKER_BASE_NAME: &str = "@vibe-box";
const TART_LINUX_BASE_NAME: &str = "vibe-tart-linux-base";
const TART_MACOS_BASE_NAME: &str = "vibe-tart-sequoia-base";
const TART_BASE_BUILDER: &str = include_str!("../../scripts/build-vibe-tart-base.sh");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TartVibeBase {
    name: &'static str,
    guest_os: &'static str,
}

#[derive(Deserialize)]
struct TartListEntry {
    #[serde(rename = "Name")]
    name: String,
}

pub async fn handle_base(command: BaseSubcommand) -> VmResult<()> {
    match command {
        BaseSubcommand::Build {
            preset,
            provider,
            guest_os,
        } => handle_build(&preset, &provider, &guest_os),
        BaseSubcommand::Validate {
            preset,
            provider,
            rebuild_docker_base,
            build_tart_base,
        } => handle_validate(&preset, &provider, rebuild_docker_base, build_tart_base),
    }
}

fn handle_build(preset: &str, provider: &str, guest_os: &str) -> VmResult<()> {
    ensure_supported_preset(preset)?;

    match provider {
        "docker" => {
            let current_exe = std::env::current_exe()
                .map_err(|e| VmError::general(e, "Failed to locate current vm executable"))?;
            let dockerfile = resolve_tool_path("Dockerfile.vibe");
            let mut command = Command::new(current_exe);
            command.args([
                "snapshot",
                "create",
                DOCKER_BASE_NAME,
                "--from-dockerfile",
                &dockerfile.to_string_lossy(),
                "--force",
            ]);
            run_command(command, "build Docker vibe base")?;
            vm_println!("Built Docker vibe base: {}", DOCKER_BASE_NAME);
        }
        "tart" => {
            let guest_os = resolve_tart_guest_os(guest_os)?;
            let base_name = tart_base_name(guest_os);
            let config = VmConfig::load(None).ok();
            build_tart_base(guest_os, base_name, config.as_ref())?;
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
        TART_MACOS_BASE_NAME
    } else {
        TART_LINUX_BASE_NAME
    }
}

fn configured_tart_vibe_base(config: &VmConfig) -> Option<TartVibeBase> {
    if config.provider.as_deref() != Some("tart") {
        return None;
    }

    let BoxSpec::String(name) = config.vm.as_ref()?.get_box_spec()? else {
        return None;
    };

    match name.as_str() {
        TART_LINUX_BASE_NAME => Some(TartVibeBase {
            name: TART_LINUX_BASE_NAME,
            guest_os: "linux",
        }),
        TART_MACOS_BASE_NAME => Some(TartVibeBase {
            name: TART_MACOS_BASE_NAME,
            guest_os: "macos",
        }),
        _ => None,
    }
}

pub(super) fn ensure_configured_tart_base(config: &VmConfig) -> VmResult<()> {
    let Some(base) = configured_tart_vibe_base(config) else {
        return Ok(());
    };

    if tart_base_exists(config, base.name)? {
        return Ok(());
    }

    vm_println!(
        "Tart vibe base '{}' is missing; building it now...",
        base.name
    );
    build_tart_base(base.guest_os, base.name, Some(config))
}

fn tart_base_exists(config: &VmConfig, base_name: &str) -> VmResult<bool> {
    let mut command = Command::new("tart");
    apply_tart_home(&mut command, config);
    let output = command
        .args(["list", "--format", "json"])
        .output()
        .map_err(|error| VmError::general(error, "Failed to list Tart bases"))?;

    if !output.status.success() {
        return Err(VmError::validation(
            "Failed to list Tart bases",
            Some("Run `tart list` to diagnose the Tart installation"),
        ));
    }

    tart_list_contains_base(&output.stdout, base_name)
}

fn tart_list_contains_base(output: &[u8], base_name: &str) -> VmResult<bool> {
    let entries: Vec<TartListEntry> = serde_json::from_slice(output)
        .map_err(|error| VmError::general(error, "Failed to parse Tart base list"))?;
    Ok(entries.iter().any(|entry| entry.name == base_name))
}

fn build_tart_base(guest_os: &str, base_name: &str, config: Option<&VmConfig>) -> VmResult<()> {
    let mut command = Command::new("bash");
    if let Some(config) = config {
        apply_tart_home(&mut command, config);
    }
    command.args([
        "-c",
        TART_BASE_BUILDER,
        "vm-tart-base-builder",
        "--guest-os",
        guest_os,
        "--name",
        base_name,
    ]);
    run_command(command, "build Tart vibe base")?;
    vm_println!("Built Tart {guest_os} vibe base: {base_name}");
    Ok(())
}

fn apply_tart_home_from_config(command: &mut Command) {
    let Ok(config) = VmConfig::load(None) else {
        return;
    };
    apply_tart_home(command, &config);
}

fn apply_tart_home(command: &mut Command, config: &VmConfig) {
    let Some(storage_path) = config
        .tart
        .as_ref()
        .and_then(|tart| tart.storage_path.as_deref())
        .filter(|path| !path.trim().is_empty())
    else {
        return;
    };

    command.env("TART_HOME", expand_tilde(storage_path));
}

fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return std::env::var("HOME").unwrap_or_else(|_| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
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
        apply_tart_home, configured_tart_vibe_base, resolve_tart_guest_os, tart_base_name,
        tart_list_contains_base, TartVibeBase, TART_BASE_BUILDER, TART_LINUX_BASE_NAME,
        TART_MACOS_BASE_NAME,
    };
    use std::ffi::OsStr;
    use std::process::Command;
    use vm_config::config::{BoxSpec, TartConfig, VmConfig, VmSettings};

    fn config(provider: &str, box_name: &str) -> VmConfig {
        VmConfig {
            provider: Some(provider.to_string()),
            vm: Some(VmSettings {
                r#box: Some(BoxSpec::String(box_name.to_string())),
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
    }

    #[test]
    fn tart_base_builder_is_embedded_in_the_binary() {
        assert!(TART_BASE_BUILDER.starts_with("#!/usr/bin/env bash"));
        assert!(TART_BASE_BUILDER.contains("tart clone"));
        assert!(TART_BASE_BUILDER.contains("--guest-os"));
    }

    #[test]
    fn invalid_tart_guest_os_is_rejected() {
        assert!(resolve_tart_guest_os("windows").is_err());
    }

    #[test]
    fn configured_vibe_bases_resolve_their_guest_os() {
        assert_eq!(
            configured_tart_vibe_base(&config("tart", TART_MACOS_BASE_NAME)),
            Some(TartVibeBase {
                name: TART_MACOS_BASE_NAME,
                guest_os: "macos"
            })
        );
        assert_eq!(
            configured_tart_vibe_base(&config("tart", TART_LINUX_BASE_NAME)),
            Some(TartVibeBase {
                name: TART_LINUX_BASE_NAME,
                guest_os: "linux"
            })
        );
    }

    #[test]
    fn custom_images_and_other_providers_do_not_auto_build() {
        assert_eq!(
            configured_tart_vibe_base(&config("tart", "ghcr.io/example/custom:latest")),
            None
        );
        assert_eq!(
            configured_tart_vibe_base(&config("docker", TART_MACOS_BASE_NAME)),
            None
        );
    }

    #[test]
    fn tart_list_detection_matches_exact_base_name() {
        let output = br#"[
            {"Name":"vibe-tart-sequoia-base","State":"stopped","Source":"local"},
            {"Name":"vm-mac","State":"stopped","Source":"local"}
        ]"#;

        assert!(tart_list_contains_base(output, TART_MACOS_BASE_NAME).unwrap());
        assert!(!tart_list_contains_base(output, TART_LINUX_BASE_NAME).unwrap());
    }

    #[test]
    fn tart_storage_path_is_forwarded_to_commands() {
        let mut command = Command::new("tart");
        let config = VmConfig {
            tart: Some(TartConfig {
                storage_path: Some("/Volumes/ExternalSSD/Tart".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };

        apply_tart_home(&mut command, &config);

        assert!(command.get_envs().any(|(key, value)| key == "TART_HOME"
            && value == Some(OsStr::new("/Volumes/ExternalSSD/Tart"))));
    }
}
