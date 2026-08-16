use std::fs::File;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use vm_core::{vm_println, vm_progress};
use vm_packages::{COMPOSE_PROJECT, TART_INSTANCE_NAME};
use vm_provider::{tart_base, TartCommand};

use crate::commands::base;
use crate::error::{VmError, VmResult};

use super::appliance::MaintenanceTask;
use super::{files::ApplianceFiles, process};

const GUEST_ROOT: &str = "/opt/vm-packages";
const READY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct TartEntry {
    #[serde(alias = "Name")]
    name: String,
    #[serde(default, alias = "State")]
    state: String,
}

struct PackageTart {
    command: TartCommand,
}

impl PackageTart {
    fn discover(files: &ApplianceFiles) -> VmResult<Self> {
        let recorded = files
            .read_state()?
            .and_then(|state| state.tart_home)
            .map(PathBuf::from);
        Ok(Self {
            command: recorded
                .map(|home| TartCommand::new(Some(home)))
                .unwrap_or_else(|| TartCommand::from_config(None)),
        })
    }

    fn storage_home(&self) -> Option<String> {
        self.command
            .home()
            .map(|path| path.to_string_lossy().into_owned())
    }

    fn find_entry(&self, name: &str) -> VmResult<Option<TartEntry>> {
        let output = process::output(
            self.command.command().args(["list", "--format", "json"]),
            "list Tart virtual machines",
        )?;
        let entries = parse_inventory(&output.stdout)
            .map_err(|error| VmError::general(error, "Failed to parse Tart VM inventory"))?;
        Ok(entries.into_iter().find(|entry| entry.name == name))
    }

    fn guest_shell(&self, script: &str) -> Command {
        let mut command = self.command.command();
        command.args(["exec", TART_INSTANCE_NAME, "bash", "-lc", script]);
        command
    }

    fn compose_command(&self, arguments: &str) -> Command {
        self.guest_shell(&format!(
            "cd {GUEST_ROOT} && sudo docker compose --project-name {COMPOSE_PROJECT} --file compose.yaml --env-file environment.env {arguments}"
        ))
    }

    fn gateway_url(&self, port: u16) -> VmResult<String> {
        let output = process::output(
            self.command.command().args(["ip", TART_INSTANCE_NAME]),
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
}

fn parse_inventory(output: &[u8]) -> serde_json::Result<Vec<TartEntry>> {
    serde_json::from_slice(output)
}

pub(super) fn storage_home(files: &ApplianceFiles) -> VmResult<Option<String>> {
    Ok(PackageTart::discover(files)?.storage_home())
}

pub(super) fn up(files: &ApplianceFiles, port: u16) -> VmResult<String> {
    let tart = PackageTart::discover(files)?;
    ensure_instance(&tart, files)?;
    ensure_docker(&tart)?;
    sync_controller_files(&tart, files)?;
    vm_progress!("Starting package infrastructure inside Tart...");
    process::run(
        &mut tart.compose_command("up --detach --remove-orphans --pull always"),
        "start the Tart package appliance",
    )?;
    tart.gateway_url(port)
}

pub(super) fn down(files: &ApplianceFiles) -> VmResult<()> {
    let tart = PackageTart::discover(files)?;
    let Some(entry) = tart.find_entry(TART_INSTANCE_NAME)? else {
        return Ok(());
    };
    if entry.state.eq_ignore_ascii_case("running") {
        if files.compose_path().exists() {
            process::run(
                &mut tart.compose_command("down --remove-orphans"),
                "stop the Tart package appliance",
            )?;
        }
        process::run(
            tart.command.command().args(["stop", TART_INSTANCE_NAME]),
            "stop the package infrastructure VM",
        )?;
    }
    Ok(())
}

pub(super) fn status(files: &ApplianceFiles) -> VmResult<String> {
    let tart = PackageTart::discover(files)?;
    let Some(entry) = tart.find_entry(TART_INSTANCE_NAME)? else {
        return Ok("missing".into());
    };
    if !entry.state.eq_ignore_ascii_case("running") || !files.compose_path().exists() {
        return Ok("stopped".into());
    }
    let output = process::output(
        &mut tart.compose_command("ps --status running --services"),
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
    let tart = PackageTart::discover(files)?;
    process::output(tart.command.command().arg("--version"), "connect to Tart")?;
    if let Some(entry) = tart.find_entry(TART_INSTANCE_NAME)? {
        if entry.state.eq_ignore_ascii_case("running") {
            process::output(
                &mut tart.guest_shell("sudo docker info >/dev/null"),
                "connect to Docker in the package infrastructure VM",
            )?;
            if files.compose_path().exists() {
                process::output(
                    &mut tart.compose_command("config --quiet"),
                    "validate the Tart package appliance definition",
                )?;
            }
        }
    } else if tart
        .find_entry(&tart_base::versioned_cache_name())?
        .is_none()
    {
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

pub(super) fn gateway_url(files: &ApplianceFiles, port: u16) -> VmResult<String> {
    PackageTart::discover(files)?.gateway_url(port)
}

pub(super) fn maintenance(files: &ApplianceFiles, task: MaintenanceTask<'_>) -> VmResult<String> {
    let tart = PackageTart::discover(files)?;
    let vm_was_running = tart
        .find_entry(TART_INSTANCE_NAME)?
        .is_some_and(|entry| entry.state.eq_ignore_ascii_case("running"));
    let services_were_running = vm_was_running && status(files)? == "running";
    ensure_instance(&tart, files)?;
    ensure_docker(&tart)?;
    sync_controller_files(&tart, files)?;

    if task.requires_pause() {
        process::run(
            &mut tart.compose_command(
                "stop gateway oci-cache registry work reviewer builder releaser rollout",
            ),
            "pause the Tart package appliance",
        )?;
    }
    let id_argument = task
        .backup_id()
        .map_or_else(String::new, |id| format!(" --env BACKUP_ID={id}"));
    let operation = process::output(
        &mut tart.compose_command(&format!(
            "run --rm --no-deps --env BACKUP_ACTION={}{} maintenance",
            task.action(),
            id_argument
        )),
        &format!("{} the Tart package appliance", task.action()),
    );
    let restart = if task.requires_pause() && services_were_running {
        process::run(
            &mut tart.compose_command("up --detach"),
            "resume the Tart package appliance",
        )
    } else if !vm_was_running {
        process::run(
            tart.command.command().args(["stop", TART_INSTANCE_NAME]),
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

fn ensure_instance(tart: &PackageTart, files: &ApplianceFiles) -> VmResult<()> {
    match tart.find_entry(TART_INSTANCE_NAME)? {
        Some(entry) if entry.state.eq_ignore_ascii_case("running") => {}
        Some(_) => start_instance(tart, files)?,
        None => {
            let base_name = base::ensure_tart_linux_base(&tart.command)?;
            vm_progress!("Creating dedicated package infrastructure VM...");
            process::run(
                tart.command
                    .command()
                    .args(["clone", &base_name, TART_INSTANCE_NAME]),
                "clone the package infrastructure VM",
            )?;
            tart.command.remember_instance(TART_INSTANCE_NAME)?;
            process::run(
                tart.command.command().args([
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
            start_instance(tart, files)?;
        }
    }
    wait_for_guest(tart)
}

fn start_instance(tart: &PackageTart, files: &ApplianceFiles) -> VmResult<()> {
    files.root().is_dir().then_some(()).ok_or_else(|| {
        VmError::validation("Package controller directory is missing", None::<String>)
    })?;
    let stdout = File::create(files.tart_log_path())?;
    let stderr = stdout.try_clone()?;
    tart.command
        .command()
        .args(["run", "--no-graphics", TART_INSTANCE_NAME])
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| VmError::general(error, "Failed to start package infrastructure VM"))?;
    Ok(())
}

fn wait_for_guest(tart: &PackageTart) -> VmResult<()> {
    let deadline = Instant::now() + READY_TIMEOUT;
    while Instant::now() < deadline {
        if tart
            .command
            .command()
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

fn ensure_docker(tart: &PackageTart) -> VmResult<()> {
    process::run(
        &mut tart.guest_shell(
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

fn sync_controller_files(tart: &PackageTart, files: &ApplianceFiles) -> VmResult<()> {
    process::run(
        &mut tart.guest_shell(&format!(
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
        (files.build_token_path(), "build-token"),
        (files.release_token_path(), "release-token"),
        (files.rollout_token_path(), "rollout-token"),
        (files.agent_signing_key_path(), "agent-signing-key"),
        (files.git_token_path(), "git-token"),
    ] {
        let content = std::fs::read(&source).map_err(|error| {
            VmError::filesystem(
                error,
                source.display().to_string(),
                "read package controller file",
            )
        })?;
        process::input(
            tart.command.command().args([
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

#[cfg(test)]
mod tests {
    use super::{format_gateway_url, parse_inventory};

    #[test]
    fn gateway_url_supports_ipv4_and_ipv6() {
        assert_eq!(
            format_gateway_url("192.0.2.4", 3080),
            "http://192.0.2.4:3080"
        );
        assert_eq!(format_gateway_url("fd00::4", 3080), "http://[fd00::4]:3080");
    }

    #[test]
    fn inventory_accepts_tart_and_legacy_field_casing() {
        let entries = parse_inventory(
            br#"[
                {"Name":"vm-packages","State":"running"},
                {"name":"legacy","state":"stopped"}
            ]"#,
        )
        .unwrap();

        assert_eq!(entries[0].name, "vm-packages");
        assert_eq!(entries[0].state, "running");
        assert_eq!(entries[1].name, "legacy");
        assert_eq!(entries[1].state, "stopped");
    }
}
