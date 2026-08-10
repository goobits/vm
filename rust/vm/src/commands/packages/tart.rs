use std::fs::File;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use vm_core::{vm_println, vm_progress};
use vm_packages::{COMPOSE_PROJECT, TART_INSTANCE_NAME};
use vm_provider::tart_base;

use crate::commands::base;
use crate::error::{VmError, VmResult};

use super::appliance::{MaintenanceTask, PackageJob};
use super::{files::ApplianceFiles, process};

const GUEST_ROOT: &str = "/opt/vm-packages";
const READY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct TartEntry {
    name: String,
    #[serde(default)]
    state: String,
}

pub(super) fn up(files: &ApplianceFiles, port: u16) -> VmResult<String> {
    ensure_instance(files)?;
    ensure_docker()?;
    sync_controller_files(files)?;
    vm_progress!("Starting package infrastructure inside Tart...");
    process::run(
        &mut compose_command("up --detach --remove-orphans --pull always"),
        "start the Tart package appliance",
    )?;
    gateway_url(port)
}

pub(super) fn down(files: &ApplianceFiles) -> VmResult<()> {
    let Some(entry) = find_entry(TART_INSTANCE_NAME)? else {
        return Ok(());
    };
    if entry.state.eq_ignore_ascii_case("running") {
        if files.compose_path().exists() {
            process::run(
                &mut compose_command("down --remove-orphans"),
                "stop the Tart package appliance",
            )?;
        }
        process::run(
            Command::new("tart").args(["stop", TART_INSTANCE_NAME]),
            "stop the package infrastructure VM",
        )?;
    }
    Ok(())
}

pub(super) fn status(files: &ApplianceFiles) -> VmResult<String> {
    let Some(entry) = find_entry(TART_INSTANCE_NAME)? else {
        return Ok("missing".into());
    };
    if !entry.state.eq_ignore_ascii_case("running") || !files.compose_path().exists() {
        return Ok("stopped".into());
    }
    let output = process::output(
        &mut compose_command("ps --status running --services"),
        "inspect the Tart package appliance",
    )?;
    Ok(if String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|service| service == "gateway")
    {
        "running"
    } else {
        "stopped"
    }
    .into())
}

pub(super) fn doctor(files: &ApplianceFiles) -> VmResult<()> {
    process::output(Command::new("tart").arg("--version"), "connect to Tart")?;
    if let Some(entry) = find_entry(TART_INSTANCE_NAME)? {
        if entry.state.eq_ignore_ascii_case("running") {
            process::output(
                &mut guest_shell("sudo docker info >/dev/null"),
                "connect to Docker in the package infrastructure VM",
            )?;
            if files.compose_path().exists() {
                process::output(
                    &mut compose_command("config --quiet"),
                    "validate the Tart package appliance definition",
                )?;
            }
        }
    } else if find_entry(&tart_base::versioned_cache_name())?.is_none() {
        return Err(VmError::validation(
            format!(
                "Tart base '{}' is missing",
                tart_base::versioned_cache_name()
            ),
            Some("Run `vm packages up`; it prepares the Linux base automatically"),
        ));
    }
    vm_println!("  Tart runtime: ready");
    Ok(())
}

pub(super) fn gateway_url(port: u16) -> VmResult<String> {
    let output = process::output(
        Command::new("tart").args(["ip", TART_INSTANCE_NAME]),
        "resolve the package infrastructure VM address",
    )?;
    let address = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if address.is_empty() {
        return Err(VmError::validation(
            "Tart returned an empty package infrastructure VM address",
            Some("Run `vm packages doctor --runtime tart`"),
        ));
    }
    Ok(format_gateway_url(&address, port))
}

pub(super) fn run_job(_files: &ApplianceFiles, job: PackageJob<'_>) -> VmResult<()> {
    process::validate_job_id(job.id())?;
    process::run(
        &mut compose_command(&format!(
            "run --rm --no-deps --env {}={} {}",
            job.variable(),
            job.id(),
            job.service(),
        )),
        "run the ephemeral package job",
    )
}

pub(super) fn maintenance(files: &ApplianceFiles, task: MaintenanceTask<'_>) -> VmResult<String> {
    let vm_was_running = find_entry(TART_INSTANCE_NAME)?
        .is_some_and(|entry| entry.state.eq_ignore_ascii_case("running"));
    let services_were_running = vm_was_running && status(files)? == "running";
    ensure_instance(files)?;
    ensure_docker()?;
    sync_controller_files(files)?;

    if task.requires_pause() {
        process::run(
            &mut compose_command("stop gateway registry work"),
            "pause the Tart package appliance",
        )?;
    }
    let id_argument = task
        .backup_id()
        .map_or_else(String::new, |id| format!(" --env BACKUP_ID={id}"));
    let operation = process::output(
        &mut compose_command(&format!(
            "run --rm --no-deps --env BACKUP_ACTION={}{} maintenance",
            task.action(),
            id_argument
        )),
        &format!("{} the Tart package appliance", task.action()),
    );
    let restart = if task.requires_pause() && services_were_running {
        process::run(
            &mut compose_command("up --detach gateway"),
            "resume the Tart package appliance",
        )
    } else if !vm_was_running {
        process::run(
            Command::new("tart").args(["stop", TART_INSTANCE_NAME]),
            "stop the package infrastructure VM after maintenance",
        )
    } else {
        Ok(())
    };
    let output = operation?;
    restart?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn format_gateway_url(address: &str, port: u16) -> String {
    if address.contains(':') {
        format!("http://[{address}]:{port}")
    } else {
        format!("http://{address}:{port}")
    }
}

fn ensure_instance(files: &ApplianceFiles) -> VmResult<()> {
    match find_entry(TART_INSTANCE_NAME)? {
        Some(entry) if entry.state.eq_ignore_ascii_case("running") => {}
        Some(_) => start_instance(files)?,
        None => {
            let base_name = base::ensure_tart_linux_base()?;
            vm_progress!("Creating dedicated package infrastructure VM...");
            process::run(
                Command::new("tart").args(["clone", &base_name, TART_INSTANCE_NAME]),
                "clone the package infrastructure VM",
            )?;
            process::run(
                Command::new("tart").args([
                    "set",
                    TART_INSTANCE_NAME,
                    "--cpu",
                    "4",
                    "--memory",
                    "8192",
                    "--disk-size",
                    "100",
                ]),
                "size the package infrastructure VM",
            )?;
            start_instance(files)?;
        }
    }
    wait_for_guest()
}

fn start_instance(files: &ApplianceFiles) -> VmResult<()> {
    files.root().is_dir().then_some(()).ok_or_else(|| {
        VmError::validation("Package controller directory is missing", None::<String>)
    })?;
    let stdout = File::create(files.tart_log_path())?;
    let stderr = stdout.try_clone()?;
    Command::new("tart")
        .args(["run", "--no-graphics", TART_INSTANCE_NAME])
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| VmError::general(error, "Failed to start package infrastructure VM"))?;
    Ok(())
}

fn wait_for_guest() -> VmResult<()> {
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if Command::new("tart")
            .args(["exec", TART_INSTANCE_NAME, "true"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
        {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(2));
    }
    Err(VmError::validation(
        "Package infrastructure VM did not become ready",
        Some("Inspect ~/.vm/infrastructure/packages/tart-run.log"),
    ))
}

fn ensure_docker() -> VmResult<()> {
    process::run(
        &mut guest_shell(
            r#"set -e
if ! command -v docker >/dev/null 2>&1; then
  curl -fsSL https://get.docker.com | sudo sh
fi
sudo systemctl enable --now docker >/dev/null 2>&1 || true
sudo docker info >/dev/null
sudo docker compose version >/dev/null"#,
        ),
        "prepare Docker inside the package infrastructure VM",
    )
}

fn sync_controller_files(files: &ApplianceFiles) -> VmResult<()> {
    process::run(
        &mut guest_shell(&format!(
            "sudo mkdir -p {GUEST_ROOT} && sudo chmod 700 {GUEST_ROOT}"
        )),
        "prepare the package controller directory in Tart",
    )?;
    for (source, name) in [
        (files.compose_path(), "compose.yaml"),
        (files.gateway_path(), "Caddyfile"),
        (files.environment_path(), "environment.env"),
        (files.read_token_path(), "read-token"),
        (files.publish_token_path(), "publish-token"),
        (files.controller_token_path(), "controller-token"),
        (files.reviewer_token_path(), "reviewer-token"),
        (files.release_token_path(), "release-token"),
        (files.rollout_token_path(), "rollout-token"),
        (files.git_token_path(), "git-token"),
        (files.ci_publish_token_path(), "ci-publish-token"),
    ] {
        let content = std::fs::read(&source).map_err(|error| {
            VmError::filesystem(
                error,
                source.display().to_string(),
                "read package controller file",
            )
        })?;
        process::input(
            Command::new("tart").args([
                "exec",
                "-i",
                TART_INSTANCE_NAME,
                "bash",
                "-lc",
                &format!(
                    "sudo tee {GUEST_ROOT}/{name} >/dev/null && sudo chmod 600 {GUEST_ROOT}/{name}"
                ),
            ]),
            &content,
            "copy package controller files into Tart",
        )?;
    }
    Ok(())
}

fn find_entry(name: &str) -> VmResult<Option<TartEntry>> {
    let output = process::output(
        Command::new("tart").args(["list", "--format", "json"]),
        "list Tart virtual machines",
    )?;
    let entries: Vec<TartEntry> = serde_json::from_slice(&output.stdout)
        .map_err(|error| VmError::general(error, "Failed to parse Tart VM inventory"))?;
    Ok(entries.into_iter().find(|entry| entry.name == name))
}

fn guest_shell(script: &str) -> Command {
    let mut command = Command::new("tart");
    command.args(["exec", TART_INSTANCE_NAME, "bash", "-lc", script]);
    command
}

fn compose_command(arguments: &str) -> Command {
    guest_shell(&format!(
        "cd {GUEST_ROOT} && sudo docker compose --project-name {COMPOSE_PROJECT} --file compose.yaml --env-file environment.env {arguments}"
    ))
}

#[cfg(test)]
mod tests {
    use super::format_gateway_url;

    #[test]
    fn gateway_url_supports_ipv4_and_ipv6() {
        assert_eq!(
            format_gateway_url("192.0.2.4", 3080),
            "http://192.0.2.4:3080"
        );
        assert_eq!(format_gateway_url("fd00::4", 3080), "http://[fd00::4]:3080");
    }
}
