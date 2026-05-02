use crate::cli::BaseSubcommand;
use crate::error::{VmError, VmResult};
use std::process::Command;
use vm_config::{config::VmConfig, resolve_tool_path, AppConfig};
use vm_core::vm_println;

const DOCKER_BASE_NAME: &str = "@vibe-box";
const TART_LINUX_BASE_NAME: &str = "vibe-tart-linux-base";
const TART_MACOS_BASE_NAME: &str = "vibe-tart-sequoia-base";

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
            let current_exe = std::env::current_exe().map_err(|e| VmError::General {
                source: Box::new(e),
                context: "Failed to locate current vm executable".to_string(),
            })?;
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
            let script = resolve_tool_path("scripts/internal/build-vibe-tart-base.sh");
            let mut command = Command::new(script);
            apply_tart_home_from_config(&mut command);
            command.args(["--guest-os", guest_os, "--name", base_name]);
            run_command(command, "build Tart vibe base")?;
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
        _ => Err(VmError::Validation {
            message: "Invalid Tart guest OS".to_string(),
            field: Some("guest-os".to_string()),
        }),
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

fn apply_tart_home_from_config(command: &mut Command) {
    let Ok(config) = VmConfig::load(None) else {
        return;
    };
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
        Err(VmError::Validation {
            message: "Only the 'vibe' base workflow is currently supported".to_string(),
            field: Some("preset".to_string()),
        })
    }
}

fn run_command(mut command: Command, context: &str) -> VmResult<()> {
    let status = command.status().map_err(|e| VmError::General {
        source: Box::new(e),
        context: format!("Failed to {context}"),
    })?;

    if status.success() {
        Ok(())
    } else {
        Err(VmError::Validation {
            message: format!("{context} failed"),
            field: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_tart_guest_os, tart_base_name};

    #[test]
    fn explicit_tart_guest_os_resolves_base_name() {
        assert_eq!(resolve_tart_guest_os("linux").unwrap(), "linux");
        assert_eq!(resolve_tart_guest_os("macos").unwrap(), "macos");
        assert_eq!(tart_base_name("linux"), "vibe-tart-linux-base");
        assert_eq!(tart_base_name("macos"), "vibe-tart-sequoia-base");
    }

    #[test]
    fn invalid_tart_guest_os_is_rejected() {
        assert!(resolve_tart_guest_os("windows").is_err());
    }
}
