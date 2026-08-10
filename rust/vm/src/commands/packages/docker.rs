use std::process::Command;

use crate::error::VmResult;
use vm_core::{vm_println, vm_progress};
use vm_packages::COMPOSE_PROJECT;

use super::appliance::{MaintenanceTask, PackageJob};
use super::{files::ApplianceFiles, process};

pub(super) fn up(files: &ApplianceFiles, port: u16) -> VmResult<String> {
    doctor(files)?;
    vm_progress!("Starting package infrastructure in Docker...");
    process::run(
        compose(files).args(["up", "--detach", "--remove-orphans", "--pull", "always"]),
        "start the Docker package appliance",
    )?;
    Ok(format!("http://127.0.0.1:{port}"))
}

pub(super) fn down(files: &ApplianceFiles) -> VmResult<()> {
    process::run(
        compose(files).args(["down", "--remove-orphans"]),
        "stop the Docker package appliance",
    )
}

pub(super) fn status(files: &ApplianceFiles) -> VmResult<String> {
    if !files.compose_path().exists() {
        return Ok("missing".to_string());
    }
    let output = process::output(
        compose(files).args(["ps", "--status", "running", "--services"]),
        "inspect the Docker package appliance",
    )?;
    let services = String::from_utf8_lossy(&output.stdout);
    Ok(if services.lines().any(|service| service == "gateway") {
        "running"
    } else {
        "stopped"
    }
    .to_string())
}

pub(super) fn doctor(files: &ApplianceFiles) -> VmResult<()> {
    process::output(Command::new("docker").arg("info"), "connect to Docker")?;
    if files.compose_path().exists() {
        process::output(
            compose(files).args(["config", "--quiet"]),
            "validate the package appliance definition",
        )?;
    }
    vm_println!("  Docker runtime: ready");
    Ok(())
}

pub(super) fn run_job(files: &ApplianceFiles, job: PackageJob<'_>) -> VmResult<()> {
    process::validate_job_id(job.id())?;
    process::run(
        compose(files)
            .args(["run", "--rm", "--no-deps", "--env"])
            .arg(format!("{}={}", job.variable(), job.id()))
            .arg(job.service()),
        "run the ephemeral package job",
    )
}

pub(super) fn maintenance(files: &ApplianceFiles, task: MaintenanceTask<'_>) -> VmResult<String> {
    doctor(files)?;
    let was_running = task.requires_pause() && status(files)? == "running";
    if task.requires_pause() {
        process::run(
            compose(files).args(["stop", "gateway", "oci-cache", "registry", "work"]),
            "pause the Docker package appliance",
        )?;
    }

    let mut command = maintenance_command(files, task);
    let operation = process::output(
        &mut command,
        &format!("{} the Docker package appliance", task.action()),
    );
    let restart = if was_running {
        process::run(
            compose(files).args(["up", "--detach", "gateway"]),
            "resume the Docker package appliance",
        )
    } else {
        Ok(())
    };
    let output = operation?;
    restart?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn maintenance_command(files: &ApplianceFiles, task: MaintenanceTask<'_>) -> Command {
    let mut command = compose(files);
    command.args(["run", "--rm", "--no-deps", "--env"]);
    command.arg(format!("BACKUP_ACTION={}", task.action()));
    if let Some(backup_id) = task.backup_id() {
        command.arg("--env").arg(format!("BACKUP_ID={backup_id}"));
    }
    command.arg("maintenance");
    command
}

fn compose(files: &ApplianceFiles) -> Command {
    let mut command = Command::new("docker");
    command
        .current_dir(files.root())
        .args(["compose", "--project-name", COMPOSE_PROJECT, "--file"])
        .arg(files.compose_path())
        .args(["--env-file"])
        .arg(files.environment_path());
    command
}
