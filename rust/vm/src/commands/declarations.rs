//! Project-owned declarations for environments created through the CLI.

use crate::error::{VmError, VmResult};
use fs2::FileExt;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use vm_config::config::{
    CpuLimit, EnvironmentDeclaration, ImageSpec, MemoryLimit, MountAccess, MountConfig,
    ProviderName, VmConfig,
};
use vm_config::validation::{validate_config, ValidationMode};
use vm_config::AppConfig;
use vm_provider::{get_provider, InstanceState};

use super::{command_context::require_project_config, vm_ops};

pub(super) fn selected_name(
    config: &VmConfig,
    requested: Option<&str>,
) -> VmResult<Option<String>> {
    if let Some(requested) = requested {
        return Ok(Some(requested.to_string()));
    }
    if config.environments.is_empty() {
        return Ok(None);
    }
    if let Some(default) = config
        .project
        .as_ref()
        .and_then(|project| project.default_environment.as_deref())
    {
        if config.environments.contains_key(default) {
            return Ok(Some(default.to_string()));
        }
        return Err(VmError::validation(
            format!("Default environment '{default}' is not declared"),
            None::<String>,
        ));
    }
    if config.environments.len() == 1 {
        return Ok(config.environments.keys().next().cloned());
    }
    let mut names = config.environments.keys().cloned().collect::<Vec<_>>();
    names.sort();
    Err(VmError::validation(
        "Multiple environments are declared without a default",
        Some(format!("Select one of: {}", names.join(", "))),
    ))
}

pub(super) struct CreateRequest {
    pub name: String,
    pub provider: String,
    pub image: Option<String>,
    pub snapshot: Option<String>,
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub mounts: Vec<String>,
    pub config_path: Option<PathBuf>,
    pub profile: Option<String>,
}

pub(super) async fn create(request: CreateRequest) -> VmResult<()> {
    validate_name(&request.name)?;
    let app = AppConfig::load(request.config_path.clone(), request.profile.clone(), None)?;
    require_project_config(&app.vm)?;
    let config_path = app
        .vm
        .owning_config_path()
        .expect("project configuration checked")
        .canonicalize()
        .map_err(VmError::from)?;
    let declaration = declaration(&request)?;
    let selected = declaration.apply_to(&app.vm);
    let report = validate_config(&selected, ValidationMode::Static).map_err(VmError::from)?;
    if report.has_errors() {
        return Err(VmError::validation(
            format!("Environment '{}' is invalid:\n{report}", request.name),
            None::<String>,
        ));
    }
    let provider = get_provider(selected.clone()).map_err(VmError::from)?;
    if !provider.supports_multi_instance() {
        return Err(VmError::validation(
            format!(
                "Provider '{}' cannot create named environments",
                request.provider
            ),
            None::<String>,
        ));
    }
    let project = app
        .vm
        .project
        .as_ref()
        .and_then(|project| project.name.as_deref())
        .unwrap();
    let runtime_name =
        vm_ops::target::canonical_instance_name(provider.name(), project, Some(&request.name));
    if provider
        .list_instances()
        .map_err(VmError::from)?
        .iter()
        .any(|instance| instance.name == runtime_name)
    {
        return Err(VmError::validation(
            format!("Environment '{}' already exists", request.name),
            None::<String>,
        ));
    }

    persist_declaration(&config_path, &request.name, declaration)?;
    vm_ops::handle_create(
        provider.clone_box(),
        selected,
        app.global,
        false,
        Some(request.name.clone()),
    )
    .await
    .map_err(|error| {
        VmError::validation(
            format!(
                "Provisioning '{}' failed; its declaration remains in vm.yaml: {error}",
                request.name
            ),
            Some(format!(
                "Fix the cause, then retry with `vm start {}`",
                request.name
            )),
        )
    })?;
    match provider
        .instance_state(Some(&runtime_name))
        .map_err(VmError::from)?
    {
        InstanceState::Running | InstanceState::Starting => {
            if let Err(error) = provider.stop(Some(&runtime_name)) {
                if provider
                    .instance_state(Some(&runtime_name))
                    .map_err(VmError::from)?
                    != InstanceState::Stopped
                {
                    return Err(VmError::from(error));
                }
            }
        }
        InstanceState::Stopped => {}
        state => {
            return Err(VmError::validation(
                format!(
                    "Environment '{}' was created but could not be left stopped ({state:?})",
                    request.name
                ),
                Some(format!("Inspect it with `vm status {}`", request.name)),
            ))
        }
    }
    println!("Environment '{}' is declared and stopped", request.name);
    Ok(())
}

fn validate_name(name: &str) -> VmResult<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(VmError::validation(
            format!("Invalid environment name '{name}'"),
            Some("Use letters, digits, dashes, and underscores"),
        ));
    }
    Ok(())
}

fn declaration(request: &CreateRequest) -> VmResult<EnvironmentDeclaration> {
    let image = match (&request.image, &request.snapshot) {
        (Some(image), None) => ImageSpec::String(image.clone()),
        (None, Some(snapshot)) => {
            ImageSpec::String(format!("@{}", snapshot.trim_start_matches('@')))
        }
        _ => {
            return Err(VmError::validation(
                "Choose exactly one image or snapshot",
                None::<String>,
            ))
        }
    };
    let cpus = request.cpu.as_deref().map(parse_cpu).transpose()?;
    let memory = request.memory.as_deref().map(parse_memory).transpose()?;
    let mounts = request
        .mounts
        .iter()
        .map(|mount| parse_mount(mount))
        .collect::<VmResult<Vec<_>>>()?;
    Ok(EnvironmentDeclaration {
        provider: ProviderName::from(request.provider.as_str()),
        image,
        services: Default::default(),
        cpus,
        memory,
        mounts,
    })
}

fn parse_cpu(value: &str) -> VmResult<CpuLimit> {
    serde_yaml_ng::from_str(value).map_err(|error| {
        VmError::validation(
            format!("Invalid CPU limit '{value}': {error}"),
            None::<String>,
        )
    })
}

fn parse_memory(value: &str) -> VmResult<MemoryLimit> {
    serde_yaml_ng::from_str(value).map_err(|error| {
        VmError::validation(
            format!("Invalid memory limit '{value}': {error}"),
            None::<String>,
        )
    })
}

fn parse_mount(value: &str) -> VmResult<MountConfig> {
    let (source, target) = value.rsplit_once(':').ok_or_else(|| {
        VmError::validation(
            format!("Invalid mount '{value}'"),
            Some("Use HOST_PATH:GUEST_PATH"),
        )
    })?;
    if source.is_empty() {
        return Err(VmError::validation(
            format!("Invalid mount '{value}'"),
            Some("Use a nonempty host path and an absolute guest path"),
        ));
    }
    vm_config::config::mounts::validate_mount_target(Path::new(target)).map_err(VmError::from)?;
    Ok(MountConfig {
        source: PathBuf::from(source),
        target: PathBuf::from(target),
        access: MountAccess::ReadWrite,
    })
}

fn persist_declaration(
    path: &Path,
    name: &str,
    declaration: EnvironmentDeclaration,
) -> VmResult<()> {
    let lock_path = path.with_extension("yaml.lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(VmError::from)?;
    lock.lock_exclusive().map_err(VmError::from)?;
    let mut raw = VmConfig::from_file(&path.to_path_buf()).map_err(VmError::from)?;
    if raw.environments.contains_key(name) {
        return Err(VmError::validation(
            format!("Environment '{name}' is already declared"),
            None::<String>,
        ));
    }
    let first = raw.environments.is_empty();
    raw.environments.insert(name.to_string(), declaration);
    if first {
        if let Some(project) = raw.project.as_mut() {
            project
                .default_environment
                .get_or_insert_with(|| name.to_string());
        }
    }
    let yaml = serde_yaml_ng::to_string(&raw)
        .map_err(|error| VmError::config(error, "serialize environment declaration"))?;
    vm_core::file_system::atomic_write(path, yaml.as_bytes())
        .map_err(|error| VmError::filesystem(error, path.display().to_string(), "write"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declaration_is_recorded_once_without_overwriting_project_identity() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vm.yaml");
        std::fs::write(
            &path,
            "version: '2.0'\nproject:\n  name: demo\nprovider: docker\n",
        )
        .unwrap();
        let declaration = EnvironmentDeclaration {
            provider: ProviderName::Docker,
            image: ImageSpec::String("debian:bookworm".into()),
            services: Default::default(),
            cpus: None,
            memory: None,
            mounts: Vec::new(),
        };
        persist_declaration(&path, "dev", declaration.clone()).unwrap();
        let raw = VmConfig::from_file(&path).unwrap();
        assert_eq!(raw.project.unwrap().name.as_deref(), Some("demo"));
        assert!(raw.environments.contains_key("dev"));
        assert!(persist_declaration(&path, "dev", declaration).is_err());
    }

    #[test]
    fn mount_arguments_preserve_host_drives_and_validate_unix_guest_targets() {
        for source in [
            "./shared",
            "C:/Users/dev/shared",
            r"C:\Users\dev\shared",
            r"\\server\share",
        ] {
            let mount = parse_mount(&format!("{source}:/packages/shared")).unwrap();
            assert_eq!(mount.source, Path::new(source));
            assert_eq!(mount.target.to_str(), Some("/packages/shared"));
            assert_eq!(mount.access, MountAccess::ReadWrite);
        }
        for value in [
            ":/shared",
            "host:relative",
            "host:/",
            "host:/etc",
            "host:/shared/../etc",
            r"host:/shared\nested",
        ] {
            assert!(parse_mount(value).is_err(), "{value}");
        }
    }

    #[test]
    fn invalid_names_and_mounts_fail_before_persistence() {
        assert!(validate_name("../other").is_err());
        assert!(parse_mount("relative:relative").is_err());
    }

    #[test]
    fn declared_environment_selection_requires_a_default_when_ambiguous() {
        let mut config = VmConfig::default();
        let one = EnvironmentDeclaration {
            provider: ProviderName::Docker,
            image: ImageSpec::String("debian:bookworm".into()),
            services: Default::default(),
            cpus: None,
            memory: None,
            mounts: Vec::new(),
        };
        config.environments.insert("dev".into(), one.clone());
        assert_eq!(
            selected_name(&config, None).unwrap().as_deref(),
            Some("dev")
        );
        config.environments.insert("test".into(), one);
        assert!(selected_name(&config, None).is_err());
        assert_eq!(
            selected_name(&config, Some("test")).unwrap().as_deref(),
            Some("test")
        );
    }
}
