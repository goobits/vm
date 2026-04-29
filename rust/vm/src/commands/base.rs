use crate::cli::BaseSubcommand;
use crate::error::{VmError, VmResult};
use std::process::Command;
use vm_config::resolve_tool_path;
use vm_core::vm_println;

const DOCKER_BASE_NAME: &str = "@vibe-box";
const TART_BASE_NAME: &str = "vibe-tart-linux-base";

pub async fn handle_base(command: BaseSubcommand) -> VmResult<()> {
    match command {
        BaseSubcommand::Build { preset, provider } => handle_build(&preset, &provider),
        BaseSubcommand::Validate {
            preset,
            provider,
            rebuild_docker_base,
            build_tart_base,
        } => handle_validate(&preset, &provider, rebuild_docker_base, build_tart_base),
    }
}

fn handle_build(preset: &str, provider: &str) -> VmResult<()> {
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
            let script = resolve_tool_path("scripts/build-vibe-tart-base.sh");
            let mut command = Command::new(script);
            command.args(["--guest-os", "linux", "--name", TART_BASE_NAME]);
            run_command(command, "build Tart vibe base")?;
            vm_println!("Built Tart vibe base: {}", TART_BASE_NAME);
        }
        _ => unreachable!(),
    }

    Ok(())
}

fn handle_validate(
    preset: &str,
    provider: &str,
    rebuild_docker_base: bool,
    build_tart_base: bool,
) -> VmResult<()> {
    ensure_supported_preset(preset)?;

    let script = resolve_tool_path("scripts/validate-vibe-providers.sh");
    let mut cmd = Command::new(script);

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
