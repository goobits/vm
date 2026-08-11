use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::error::VmResult;
use vm_core::{vm_println, vm_progress, vm_warning};
use vm_packages::{sha256_reader, ApplianceConfig, COMPOSE_PROJECT};

use super::appliance::{MaintenanceTask, PackageJob};
use super::{files::ApplianceFiles, process};

const SOURCE_FINGERPRINT_LABEL: &str = "org.goobits.vm.controller-binary-sha256";

struct SourceBuild {
    workspace: PathBuf,
    fingerprint: String,
}

#[derive(Deserialize)]
struct ImageInspect {
    #[serde(rename = "Config")]
    config: Option<ImageConfig>,
}

#[derive(Deserialize)]
struct ImageConfig {
    #[serde(rename = "Labels")]
    labels: Option<BTreeMap<String, String>>,
}

pub(super) fn up(
    files: &ApplianceFiles,
    config: &ApplianceConfig,
    allow_source_build: bool,
) -> VmResult<String> {
    doctor(files)?;
    ensure_images(config, allow_source_build)?;
    vm_progress!("Starting package infrastructure in Docker...");
    process::run(&mut up_command(files), "start the Docker package appliance")?;
    Ok(format!("http://127.0.0.1:{}", config.gateway_port))
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

fn up_command(files: &ApplianceFiles) -> Command {
    let mut command = compose(files);
    command.args(["up", "--detach", "--remove-orphans", "--pull", "missing"]);
    command
}

fn ensure_images(config: &ApplianceConfig, allow_source_build: bool) -> VmResult<()> {
    let source = discover_source_build()?;
    ensure_image(
        &config.registry_image,
        source.as_ref(),
        allow_source_build,
        "vm-package-server/docker/server/Dockerfile",
    )?;
    ensure_image(
        &config.job_image,
        source.as_ref(),
        allow_source_build,
        "vm-package-jobs/Dockerfile",
    )
}

fn ensure_image(
    image: &str,
    source: Option<&SourceBuild>,
    allow_source_fallback: bool,
    dockerfile: &str,
) -> VmResult<()> {
    if let Some(inspect) = image_inspect(image)? {
        let source_built = inspect
            .config
            .as_ref()
            .and_then(|config| config.labels.as_ref())
            .is_some_and(|labels| labels.contains_key(SOURCE_FINGERPRINT_LABEL));
        if let Some(source) = source.filter(|_| source_built || is_local_source_image(image)) {
            vm_progress!("Refreshing local package image {image} through Docker's build cache...");
            return build_source_image(source, dockerfile, image);
        }
        return Ok(());
    }

    vm_progress!("Pulling package appliance image {image}...");
    let mut pull = Command::new("docker");
    pull.args(["pull", image]);
    match process::output(&mut pull, &format!("pull package appliance image {image}")) {
        Ok(_) => Ok(()),
        Err(pull_error) => {
            if !allow_source_fallback {
                return Err(pull_error);
            }
            let Some(source) = source else {
                return Err(pull_error);
            };
            vm_warning!("Release image {image} is unavailable; building it from source");
            build_source_image(source, dockerfile, image)
        }
    }
}

fn is_local_source_image(image: &str) -> bool {
    !image.contains('@')
        && image
            .rsplit_once(':')
            .is_some_and(|(_, tag)| tag.ends_with("-local"))
}

fn build_source_image(source: &SourceBuild, dockerfile: &str, image: &str) -> VmResult<()> {
    let mut build = source_build_command(&source.workspace, dockerfile, image, &source.fingerprint);
    process::run(
        &mut build,
        &format!("build package appliance image {image}"),
    )
}

fn image_inspect(image: &str) -> VmResult<Option<ImageInspect>> {
    let output = Command::new("docker")
        .args(["image", "inspect", image])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let mut images: Vec<ImageInspect> = serde_json::from_slice(&output.stdout)?;
    Ok(images.pop())
}

fn source_build_command(
    workspace: &Path,
    dockerfile: &str,
    image: &str,
    fingerprint: &str,
) -> Command {
    let mut command = Command::new("docker");
    command
        .current_dir(workspace)
        .args(["build", "--label"])
        .arg(format!("{SOURCE_FINGERPRINT_LABEL}={fingerprint}"))
        .args(["--tag", image, "--file", dockerfile, "."]);
    command
}

fn discover_source_build() -> VmResult<Option<SourceBuild>> {
    let Ok(executable) = std::env::current_exe() else {
        return Ok(None);
    };
    let Some(workspace) = source_workspace_for_executable(&executable) else {
        return Ok(None);
    };
    let resolved = fs::canonicalize(&executable).unwrap_or(executable);
    let (fingerprint, _) = sha256_reader(BufReader::new(File::open(resolved)?))?;
    Ok(Some(SourceBuild {
        workspace,
        fingerprint,
    }))
}

fn source_workspace_for_executable(executable: &Path) -> Option<PathBuf> {
    let resolved = fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf());
    source_workspace_from(&resolved)
}

fn source_workspace_from(executable: &Path) -> Option<PathBuf> {
    executable.ancestors().find_map(|ancestor| {
        let workspace = ancestor.join("rust");
        (workspace.join("Cargo.toml").is_file()
            && workspace
                .join("vm-package-server/docker/server/Dockerfile")
                .is_file()
            && workspace.join("vm-package-jobs/Dockerfile").is_file())
        .then_some(workspace)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        is_local_source_image, source_build_command, source_workspace_for_executable,
        source_workspace_from, up_command,
    };
    use crate::commands::packages::files::ApplianceFiles;
    use std::fs;

    #[test]
    fn startup_reuses_present_immutable_images() {
        let directory = tempfile::tempdir().unwrap();
        let files = ApplianceFiles::at(directory.path().join("packages"));
        let command = up_command(&files);
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert!(arguments
            .windows(2)
            .any(|arguments| arguments == ["--pull", "missing"]));
    }

    #[test]
    fn source_workspace_is_discovered_without_an_embedded_host_path() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("checkout/rust");
        fs::create_dir_all(workspace.join("vm-package-server/docker/server")).unwrap();
        fs::create_dir_all(workspace.join("vm-package-jobs")).unwrap();
        fs::write(workspace.join("Cargo.toml"), "[workspace]").unwrap();
        fs::write(
            workspace.join("vm-package-server/docker/server/Dockerfile"),
            "FROM scratch",
        )
        .unwrap();
        fs::write(workspace.join("vm-package-jobs/Dockerfile"), "FROM scratch").unwrap();

        let executable = workspace.join("target-macos-aarch64/source-install/vm");
        assert_eq!(source_workspace_from(&executable), Some(workspace));
    }

    #[cfg(unix)]
    #[test]
    fn source_workspace_resolves_an_installed_symlink() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("checkout/rust");
        fs::create_dir_all(workspace.join("vm-package-server/docker/server")).unwrap();
        fs::create_dir_all(workspace.join("vm-package-jobs")).unwrap();
        fs::write(workspace.join("Cargo.toml"), "[workspace]").unwrap();
        fs::write(
            workspace.join("vm-package-server/docker/server/Dockerfile"),
            "FROM scratch",
        )
        .unwrap();
        fs::write(workspace.join("vm-package-jobs/Dockerfile"), "FROM scratch").unwrap();
        let executable = workspace.join("target/source-install/vm");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, "binary").unwrap();
        let installed = directory.path().join("bin/vm");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        symlink(&executable, &installed).unwrap();

        assert_eq!(
            source_workspace_for_executable(&installed),
            Some(fs::canonicalize(workspace).unwrap())
        );
    }

    #[test]
    fn source_image_build_uses_structural_docker_arguments() {
        let workspace = std::path::Path::new("/checkout/rust");
        let command = source_build_command(
            workspace,
            "vm-package-jobs/Dockerfile",
            "registry.example/jobs:1",
            "abc123",
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(command.get_current_dir(), Some(workspace));
        assert_eq!(
            arguments,
            [
                "build",
                "--label",
                "org.goobits.vm.controller-binary-sha256=abc123",
                "--tag",
                "registry.example/jobs:1",
                "--file",
                "vm-package-jobs/Dockerfile",
                ".",
            ]
        );
    }

    #[test]
    fn local_source_image_tags_are_explicit() {
        assert!(is_local_source_image("vm-package-jobs:5.0.1-local"));
        assert!(is_local_source_image(
            "registry.example:5000/vm-package-jobs:dev-local"
        ));
        assert!(!is_local_source_image("vm-package-jobs:5.0.1"));
        assert!(!is_local_source_image(
            "vm-package-jobs@sha256:0123456789abcdef"
        ));
    }
}
