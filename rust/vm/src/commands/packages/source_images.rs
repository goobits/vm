use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;
use vm_core::{vm_progress, vm_warning};
use vm_packages::ApplianceConfig;
use vm_provider::ContainerEngine;

use crate::error::{VmError, VmResult};

use super::process;

const SOURCE_BUILD_LABEL: &str = "org.goobits.vm.source-build";
const SOURCE_FINGERPRINT_LABEL: &str = "org.goobits.vm.source-fingerprint";
const SOURCE_FINGERPRINT_REVISION: &str = "1";
const SOURCE_BUILD_PROFILE: &str = "source-install";

#[derive(Deserialize)]
struct ImageInspect {
    #[serde(rename = "Id")]
    id: Option<String>,
    #[serde(rename = "Config")]
    config: Option<ImageConfig>,
}

#[derive(Deserialize)]
struct ImageConfig {
    #[serde(rename = "Labels")]
    labels: Option<BTreeMap<String, String>>,
}

pub(super) fn ensure(
    engine: ContainerEngine,
    config: &ApplianceConfig,
    allow_source_build: bool,
) -> VmResult<()> {
    let source = discover_source_workspace();
    ensure_image(
        engine,
        &config.registry_image,
        source.as_deref(),
        allow_source_build,
        "vm-package-server/docker/server/Dockerfile",
    )?;
    ensure_image(
        engine,
        &config.job_image,
        source.as_deref(),
        allow_source_build,
        "vm-package-jobs/Dockerfile",
    )
}

pub(super) fn identity(engine: ContainerEngine, image: &str) -> VmResult<String> {
    Ok(inspect(engine, image)?
        .and_then(|inspect| inspect.id)
        .filter(|identity| !identity.trim().is_empty())
        .unwrap_or_else(|| image.to_string()))
}

fn ensure_image(
    engine: ContainerEngine,
    image: &str,
    source: Option<&Path>,
    allow_source_fallback: bool,
    dockerfile: &str,
) -> VmResult<()> {
    let fingerprint = source
        .map(|source| source_fingerprint(source, dockerfile))
        .transpose()?;
    if let Some(inspect) = inspect(engine, image)? {
        let source_built = is_source_built(&inspect);
        if let Some(source) = source.filter(|_| source_built || is_local_source_image(image)) {
            if fingerprint.as_deref() == image_source_fingerprint(&inspect) {
                return Ok(());
            }
            vm_progress!(
                "Refreshing local package image {image} through {}'s build cache...",
                engine.name()
            );
            return build_source_image(engine, source, dockerfile, image, fingerprint.as_deref());
        }
        return Ok(());
    }

    vm_progress!("Pulling package appliance image {image}...");
    let mut pull = Command::new(engine.executable());
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
            build_source_image(engine, source, dockerfile, image, fingerprint.as_deref())
        }
    }
}

fn is_local_source_image(image: &str) -> bool {
    !image.contains('@')
        && image
            .rsplit_once(':')
            .is_some_and(|(_, tag)| tag.ends_with("-local"))
}

fn build_source_image(
    engine: ContainerEngine,
    source: &Path,
    dockerfile: &str,
    image: &str,
    fingerprint: Option<&str>,
) -> VmResult<()> {
    process::run(
        &mut source_build_command(engine, source, dockerfile, image, fingerprint),
        &format!("build package appliance image {image}"),
    )
}

fn inspect(engine: ContainerEngine, image: &str) -> VmResult<Option<ImageInspect>> {
    let output = Command::new(engine.executable())
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

fn is_source_built(inspect: &ImageInspect) -> bool {
    inspect
        .config
        .as_ref()
        .and_then(|config| config.labels.as_ref())
        .is_some_and(|labels| labels.get(SOURCE_BUILD_LABEL).map(String::as_str) == Some("true"))
}

fn image_source_fingerprint(inspect: &ImageInspect) -> Option<&str> {
    inspect
        .config
        .as_ref()?
        .labels
        .as_ref()?
        .get(SOURCE_FINGERPRINT_LABEL)
        .map(String::as_str)
}

fn source_fingerprint(workspace: &Path, dockerfile: &str) -> VmResult<String> {
    let inputs = if dockerfile == "vm-package-jobs/Dockerfile" {
        let mut inputs = vec![
            workspace.join("Cargo.toml"),
            workspace.join("Cargo.lock"),
            workspace.join(".cargo"),
            workspace.join("vm-package-jobs"),
            workspace.join("vm-package-git-askpass"),
            workspace.join("vm-packages"),
        ];
        if let Some(root) = workspace.parent() {
            inputs.push(root.join(".dockerignore"));
        }
        inputs
    } else {
        let mut inputs = vec![workspace.to_path_buf()];
        if let Some(root) = workspace.parent() {
            inputs.push(root.join("configs"));
            inputs.push(root.join(".dockerignore"));
        }
        inputs
    };
    let base = workspace.parent().unwrap_or(workspace);
    let mut files = Vec::new();
    for input in inputs {
        if !input.exists() {
            continue;
        }
        if input.is_file() {
            files.push(input);
            continue;
        }
        for entry in walkdir::WalkDir::new(&input)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0 || !matches!(entry.file_name().to_str(), Some("target" | ".git"))
            })
        {
            let entry = entry.map_err(|error| {
                VmError::validation(
                    format!("Failed to fingerprint {}: {error}", input.display()),
                    None::<String>,
                )
            })?;
            if entry.file_type().is_file() || entry.file_type().is_symlink() {
                files.push(entry.into_path());
            }
        }
    }
    files.sort();
    files.dedup();
    let mut material = format!(
        "revision={SOURCE_FINGERPRINT_REVISION}\0profile={SOURCE_BUILD_PROFILE}\0dockerfile={dockerfile}\0"
    )
    .into_bytes();
    for file in files {
        let relative = file.strip_prefix(base).unwrap_or(&file).to_string_lossy();
        let metadata = fs::symlink_metadata(&file)?;
        let content = if metadata.file_type().is_symlink() {
            fs::read_link(&file)?
                .to_string_lossy()
                .into_owned()
                .into_bytes()
        } else {
            fs::read(&file)?
        };
        material.extend_from_slice(relative.as_bytes());
        material.push(0);
        material.extend_from_slice(content.len().to_string().as_bytes());
        material.push(0);
        material.extend_from_slice(&content);
        material.push(0xff);
    }
    Ok(vm_packages::sha256_hex(material))
}

fn source_build_command(
    engine: ContainerEngine,
    workspace: &Path,
    dockerfile: &str,
    image: &str,
    fingerprint: Option<&str>,
) -> Command {
    let (context, dockerfile) = workspace
        .parent()
        .filter(|root| {
            workspace.file_name().is_some_and(|name| name == "rust")
                && root.join("configs/defaults.yaml").is_file()
        })
        .map_or_else(
            || (workspace.to_path_buf(), dockerfile.to_string()),
            |root| (root.to_path_buf(), format!("rust/{dockerfile}")),
        );
    let mut command = Command::new(engine.executable());
    let build_profile = format!("VM_PACKAGE_BUILD_PROFILE={SOURCE_BUILD_PROFILE}");
    command.current_dir(context).arg("build");
    if matches!(engine, ContainerEngine::Docker) {
        command.arg("--provenance=false");
    }
    command
        .arg("--label")
        .arg(format!("{SOURCE_BUILD_LABEL}=true"))
        .args(["--build-arg", build_profile.as_str()]);
    if let Some(fingerprint) = fingerprint {
        command
            .arg("--label")
            .arg(format!("{SOURCE_FINGERPRINT_LABEL}={fingerprint}"));
    }
    command.args(["--tag", image, "--file", dockerfile.as_str(), "."]);
    command
}

fn discover_source_workspace() -> Option<PathBuf> {
    source_workspace_for_executable(&std::env::current_exe().ok()?)
}

fn source_workspace_for_executable(executable: &Path) -> Option<PathBuf> {
    let resolved = fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf());
    source_workspace_from(&resolved)
}

fn source_workspace_from(executable: &Path) -> Option<PathBuf> {
    executable.ancestors().find_map(|ancestor| {
        source_workspace_at(ancestor).or_else(|| {
            let marker = ancestor.join(vm_core::SOURCE_WORKSPACE_MARKER);
            let source = fs::read_to_string(marker).ok()?;
            source_workspace_at(&fs::canonicalize(source.trim()).ok()?)
        })
    })
}

fn source_workspace_at(path: &Path) -> Option<PathBuf> {
    let workspace = if path.join("Cargo.toml").is_file() {
        path.to_path_buf()
    } else {
        path.join("rust")
    };
    (workspace.join("Cargo.toml").is_file()
        && workspace
            .join("vm-package-server/docker/server/Dockerfile")
            .is_file()
        && workspace.join("vm-package-jobs/Dockerfile").is_file())
    .then_some(workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(root: &Path) -> PathBuf {
        let workspace = root.join("checkout/rust");
        fs::create_dir_all(workspace.join("vm-package-server/docker/server")).unwrap();
        fs::create_dir_all(workspace.join("vm-package-jobs")).unwrap();
        fs::write(workspace.join("Cargo.toml"), "[workspace]").unwrap();
        fs::write(
            workspace.join("vm-package-server/docker/server/Dockerfile"),
            "FROM scratch",
        )
        .unwrap();
        fs::write(workspace.join("vm-package-jobs/Dockerfile"), "FROM scratch").unwrap();
        workspace
    }

    #[test]
    fn source_workspace_is_discovered_without_an_embedded_host_path() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = workspace(directory.path());
        let discovered =
            source_workspace_from(&workspace.join("target/source-install/vm")).unwrap();
        assert_eq!(
            fs::canonicalize(discovered).unwrap(),
            fs::canonicalize(workspace).unwrap()
        );
    }

    #[test]
    fn source_workspace_is_recovered_from_external_build_cache() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = workspace(directory.path());
        let target = directory.path().join("tmp/vm-rust-target");
        let executable = target.join("source-install/vm");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(
            target.join(vm_core::SOURCE_WORKSPACE_MARKER),
            workspace.to_string_lossy().as_bytes(),
        )
        .unwrap();

        assert_eq!(
            source_workspace_from(&executable),
            Some(fs::canonicalize(workspace).unwrap())
        );
    }

    #[cfg(unix)]
    #[test]
    fn source_workspace_resolves_an_installed_symlink() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let workspace = workspace(directory.path());
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
        let workspace = Path::new("/checkout/rust");
        let command = source_build_command(
            ContainerEngine::Docker,
            workspace,
            "vm-package-jobs/Dockerfile",
            "registry.example/jobs:1",
            Some("abc123"),
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
                "--provenance=false",
                "--label",
                "org.goobits.vm.source-build=true",
                "--build-arg",
                "VM_PACKAGE_BUILD_PROFILE=source-install",
                "--label",
                "org.goobits.vm.source-fingerprint=abc123",
                "--tag",
                "registry.example/jobs:1",
                "--file",
                "vm-package-jobs/Dockerfile",
                ".",
            ]
        );
    }

    #[test]
    fn source_image_build_includes_repository_configuration_assets() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("checkout");
        let workspace = root.join("rust");
        fs::create_dir_all(root.join("configs")).unwrap();
        fs::create_dir_all(&workspace).unwrap();
        fs::write(root.join("configs/defaults.yaml"), "version: '2.0'\n").unwrap();

        let command = source_build_command(
            ContainerEngine::Docker,
            &workspace,
            "vm-package-server/docker/server/Dockerfile",
            "registry.example/server:1",
            None,
        );
        let arguments = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(command.get_current_dir(), Some(root.as_path()));
        assert!(arguments.windows(2).any(|arguments| {
            arguments == ["--file", "rust/vm-package-server/docker/server/Dockerfile"]
        }));
        assert!(arguments.windows(2).any(|arguments| {
            arguments == ["--build-arg", "VM_PACKAGE_BUILD_PROFILE=source-install"]
        }));
    }

    #[test]
    fn job_fingerprint_ignores_unrelated_workflow_server_changes() {
        let directory = tempfile::tempdir().unwrap();
        let workspace = directory.path().join("rust");
        for path in [
            "vm-package-jobs/src",
            "vm-package-git-askpass/src",
            "vm-packages/src",
            "vm-package-work/src",
        ] {
            fs::create_dir_all(workspace.join(path)).unwrap();
        }
        for path in [
            "Cargo.toml",
            "Cargo.lock",
            "vm-package-jobs/Dockerfile",
            "vm-package-jobs/src/lib.rs",
            "vm-package-git-askpass/Cargo.toml",
            "vm-package-git-askpass/src/main.rs",
            "vm-packages/Cargo.toml",
            "vm-packages/src/lib.rs",
            "vm-package-work/src/submission.rs",
        ] {
            fs::write(workspace.join(path), path).unwrap();
        }

        let before = source_fingerprint(&workspace, "vm-package-jobs/Dockerfile").unwrap();
        fs::write(
            workspace.join("vm-package-work/src/submission.rs"),
            "workflow-only change",
        )
        .unwrap();
        let after = source_fingerprint(&workspace, "vm-package-jobs/Dockerfile").unwrap();
        assert_eq!(before, after);

        fs::write(workspace.join("vm-package-jobs/src/lib.rs"), "job change").unwrap();
        assert_ne!(
            after,
            source_fingerprint(&workspace, "vm-package-jobs/Dockerfile").unwrap()
        );
    }

    #[test]
    fn source_image_marker_is_stable() {
        let inspect = |labels| ImageInspect {
            id: None,
            config: Some(ImageConfig {
                labels: Some(labels),
            }),
        };
        assert!(is_source_built(&inspect(BTreeMap::from([(
            SOURCE_BUILD_LABEL.into(),
            "true".into(),
        )]))));
        let fingerprint = inspect(BTreeMap::from([
            (SOURCE_BUILD_LABEL.into(), "true".into()),
            (SOURCE_FINGERPRINT_LABEL.into(), "abc123".into()),
        ]));
        assert_eq!(image_source_fingerprint(&fingerprint), Some("abc123"));
        assert!(!is_source_built(&inspect(BTreeMap::from([(
            SOURCE_BUILD_LABEL.into(),
            "false".into(),
        )]))));
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
