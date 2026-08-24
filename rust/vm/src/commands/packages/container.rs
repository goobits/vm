use std::process::Command;

use crate::error::VmResult;
use vm_core::{vm_println, vm_progress};
use vm_packages::{ApplianceConfig, COMPOSE_PROJECT};
use vm_provider::container::ContainerEngine;

use super::appliance::MaintenanceTask;
use super::{files::ApplianceFiles, process, source_images};

pub(super) fn up(
    engine: ContainerEngine,
    files: &ApplianceFiles,
    config: &ApplianceConfig,
    allow_source_build: bool,
) -> VmResult<String> {
    doctor(engine, files)?;
    source_images::ensure(engine, config, allow_source_build)?;
    vm_progress!("Starting package infrastructure with {}...", engine.name());
    process::run(
        &mut up_command(engine, files),
        "start the package appliance",
    )?;
    Ok(format!("http://127.0.0.1:{}", config.gateway_port))
}

pub(super) fn down(engine: ContainerEngine, files: &ApplianceFiles) -> VmResult<()> {
    process::run(
        compose(engine, files).args(["down", "--remove-orphans"]),
        "stop the package appliance",
    )
}

pub(super) fn status(engine: ContainerEngine, files: &ApplianceFiles) -> VmResult<String> {
    if !files.compose_path().exists() {
        return Ok("missing".to_string());
    }
    let output = process::output(
        compose(engine, files).args(["ps", "--status", "running", "--services"]),
        "inspect the package appliance",
    )?;
    let services = String::from_utf8_lossy(&output.stdout);
    Ok(if services.lines().any(|service| service == "gateway") {
        "running"
    } else {
        "stopped"
    }
    .to_string())
}

pub(super) fn doctor(engine: ContainerEngine, files: &ApplianceFiles) -> VmResult<()> {
    process::output(
        Command::new(engine.executable()).arg("info"),
        "connect to the container engine",
    )?;
    if files.compose_path().exists() {
        process::output(
            compose(engine, files).args(["config", "--quiet"]),
            "validate the package appliance definition",
        )?;
    }
    vm_println!("  {} engine: ready", engine.name());
    Ok(())
}

pub(super) fn maintenance(
    engine: ContainerEngine,
    files: &ApplianceFiles,
    task: MaintenanceTask<'_>,
) -> VmResult<String> {
    doctor(engine, files)?;
    let was_running = task.requires_pause() && status(engine, files)? == "running";
    if task.requires_pause() {
        process::run(
            compose(engine, files).args([
                "stop",
                "gateway",
                "oci-cache",
                "registry",
                "work",
                "build-edge",
                "reviewer",
                "builder",
                "releaser",
                "rollout",
            ]),
            "pause the package appliance",
        )?;
    }

    let mut command = maintenance_command(engine, files, task);
    let operation = process::output(
        &mut command,
        &format!("{} the package appliance", task.action()),
    );
    let restart = if was_running {
        process::run(
            compose(engine, files).args(["up", "--detach"]),
            "resume the package appliance",
        )
    } else {
        Ok(())
    };
    let output = operation?;
    restart?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn maintenance_command(
    engine: ContainerEngine,
    files: &ApplianceFiles,
    task: MaintenanceTask<'_>,
) -> Command {
    let mut command = compose(engine, files);
    command.args(["run", "--rm", "--no-deps", "--env"]);
    command.arg(format!("BACKUP_ACTION={}", task.action()));
    if let Some(backup_id) = task.backup_id() {
        command.arg("--env").arg(format!("BACKUP_ID={backup_id}"));
    }
    command.arg("maintenance");
    command
}

fn compose(engine: ContainerEngine, files: &ApplianceFiles) -> Command {
    let project = std::env::var("VM_PACKAGES_COMPOSE_PROJECT")
        .unwrap_or_else(|_| COMPOSE_PROJECT.to_string());
    let mut command = engine.compose_command();
    command
        .current_dir(files.root())
        .args(["--project-name", &project, "--file"])
        .arg(files.compose_path())
        .args(["--env-file"])
        .arg(files.environment_path());
    command
}

fn up_command(engine: ContainerEngine, files: &ApplianceFiles) -> Command {
    let mut command = compose(engine, files);
    command.args(["up", "--detach", "--remove-orphans", "--pull", "missing"]);
    command
}

#[cfg(test)]
mod tests {
    use super::up_command;
    use crate::commands::packages::files::ApplianceFiles;
    use vm_provider::container::ContainerEngine;

    #[test]
    fn startup_reuses_present_immutable_images() {
        let directory = tempfile::tempdir().unwrap();
        let files = ApplianceFiles::at(directory.path().join("packages"));
        let command = up_command(ContainerEngine::Docker, &files);
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(arguments
            .windows(2)
            .any(|arguments| arguments == ["--pull", "missing"]));
    }
}
