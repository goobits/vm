use crate::error::{VmError, VmResult};
use fs2::FileExt;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

type Registry = BTreeMap<String, PathBuf>;

fn registry_path() -> VmResult<PathBuf> {
    Ok(vm_core::user_paths::user_config_dir()
        .map_err(VmError::from)?
        .join("projects.json"))
}

fn read_registry(path: &Path) -> VmResult<Registry> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            VmError::general(
                error,
                format!("Invalid project registry {}", path.display()),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Registry::new()),
        Err(error) => Err(VmError::filesystem(
            error,
            path.display().to_string(),
            "read",
        )),
    }
}

fn project_id(root: &Path) -> VmResult<String> {
    let basename = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            VmError::validation("Project directory needs a UTF-8 name", None::<String>)
        })?;
    let id = basename
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if id.is_empty() {
        return Err(VmError::validation(
            "Project directory needs a usable name",
            None::<String>,
        ));
    }
    Ok(id)
}

pub(super) fn init(path: Option<PathBuf>, preset: &str, profile: Option<String>) -> VmResult<()> {
    let root = path.unwrap_or(std::env::current_dir().map_err(VmError::from)?);
    let root = root.canonicalize().map_err(VmError::from)?;
    if !root.is_dir() {
        return Err(VmError::validation(
            format!("Project path is not a directory: {}", root.display()),
            None::<String>,
        ));
    }
    let id = project_id(&root)?;
    let config = root.join("vm.yaml");
    let registry = registry_path()?;
    let defaults = vm_config::GlobalConfig::load()?.defaults;
    let contents = initial_config(&id, &root, preset, profile.as_deref(), &defaults)?;
    register_new_project(&id, &config, &registry, &contents)?;
    println!(
        "Initialized project {id} at {} (preset: {preset}, environment: dev)",
        config.display()
    );
    println!("Run `vm shell` to provision, start, and connect.");
    Ok(())
}

fn initial_config(
    id: &str,
    root: &Path,
    preset: &str,
    profile: Option<&str>,
    defaults: &vm_config::GlobalDefaults,
) -> VmResult<String> {
    use vm_config::config::{EnvironmentDeclaration, ImageSpec, ProviderName};
    let mut config =
        vm_config::PresetDetector::new(root.to_path_buf()).load_preset_resolved(preset, None)?;
    vm_config::apply_user_defaults(&mut config, defaults);
    if let Some(profile) = vm_config::AppConfig::resolve_profile_name(&config, profile, None) {
        config = vm_config::apply_profile(config, &profile)?;
        config.default_profile = Some(profile);
    }
    config.preset = Some(preset.into());
    let provider = config.provider.clone().unwrap_or(ProviderName::Docker);
    let settings = config.vm.get_or_insert_with(Default::default);
    let image = settings
        .image
        .clone()
        .unwrap_or_else(|| ImageSpec::String("ubuntu:24.04".into()));
    settings.image = Some(image.clone());
    config.provider = Some(provider.clone());
    let project = config.project.get_or_insert_with(Default::default);
    project.name = Some(id.into());
    project.default_environment = Some("dev".into());
    project
        .workspace_path
        .get_or_insert_with(|| "/workspace".into());
    config.environments.insert(
        "dev".into(),
        EnvironmentDeclaration {
            provider,
            image,
            services: Default::default(),
            cpus: None,
            memory: None,
            mounts: Vec::new(),
        },
    );
    let report = vm_config::validation::validate_config(
        &config,
        vm_config::validation::ValidationMode::Static,
    )?;
    if report.has_errors() {
        return Err(VmError::validation(
            format!("Invalid initialization configuration:\n{report}"),
            None::<String>,
        ));
    }
    serde_yaml_ng::to_string(&config)
        .map_err(|error| VmError::config(error, "serialize initial configuration"))
}

fn register_new_project(id: &str, config: &Path, registry: &Path, contents: &str) -> VmResult<()> {
    if config.exists() {
        return Err(VmError::validation(
            format!("Project configuration already exists: {}", config.display()),
            None::<String>,
        ));
    }
    let parent = registry.parent().ok_or_else(|| {
        VmError::validation("Project registry has no parent directory", None::<String>)
    })?;
    fs::create_dir_all(parent).map_err(VmError::from)?;
    let lock_path = registry.with_extension("lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .map_err(VmError::from)?;
    lock.lock_exclusive().map_err(VmError::from)?;
    let mut entries = read_registry(registry)?;
    if let Some(existing) = entries
        .get(id)
        .filter(|existing| existing.as_path() != config)
    {
        return Err(VmError::validation(
            format!(
                "Project ID '{id}' is already registered at {}",
                existing.display()
            ),
            None::<String>,
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(config)
        .map_err(|error| VmError::filesystem(error, config.display().to_string(), "create"))?;
    if let Err(error) = file
        .write_all(contents.as_bytes())
        .and_then(|_| file.sync_all())
    {
        let _ = fs::remove_file(config);
        return Err(VmError::filesystem(
            error,
            config.display().to_string(),
            "write",
        ));
    }
    entries.insert(id.to_string(), config.to_path_buf());
    let serialized = serde_json::to_vec_pretty(&entries).map_err(VmError::from)?;
    if let Err(error) = vm_core::file_system::atomic_write(registry, &serialized) {
        let _ = fs::remove_file(config);
        return Err(VmError::filesystem(
            error,
            registry.display().to_string(),
            "write",
        ));
    }
    Ok(())
}

pub(super) fn resolve(selector: &Path) -> VmResult<PathBuf> {
    if selector.exists() {
        let path = if selector.is_dir() {
            selector.join("vm.yaml")
        } else {
            selector.to_path_buf()
        };
        if !path.is_file() {
            return Err(VmError::validation(
                format!("Project configuration not found: {}", path.display()),
                None::<String>,
            ));
        }
        return path.canonicalize().map_err(VmError::from);
    }
    let id = selector
        .to_str()
        .ok_or_else(|| VmError::validation("Project ID needs to be UTF-8", None::<String>))?;
    let path = read_registry(&registry_path()?)?
        .remove(id)
        .ok_or_else(|| {
            VmError::validation(
                format!("Unknown project '{id}'"),
                Some("Run vm init in the project directory"),
            )
        })?;
    if !path.is_file() {
        return Err(VmError::validation(
            format!(
                "Registered project '{id}' has no configuration at {}",
                path.display()
            ),
            None::<String>,
        ));
    }
    path.canonicalize().map_err(VmError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registration_creates_config_and_rejects_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("vm.yaml");
        let registry = directory.path().join("registry.json");
        register_new_project("example", &config, &registry, "project:\n  name: example\n").unwrap();
        assert!(fs::read_to_string(&config)
            .unwrap()
            .contains("name: example"));
        assert_eq!(read_registry(&registry).unwrap()["example"], config);
        assert!(
            register_new_project("example", &config, &registry, "project:\n  name: example\n")
                .is_err()
        );
    }

    #[test]
    fn registration_recovers_deleted_config_without_claiming_another_project() {
        let directory = tempfile::tempdir().unwrap();
        let config = directory.path().join("vm.yaml");
        let registry = directory.path().join("registry.json");
        register_new_project("example", &config, &registry, "original").unwrap();
        fs::remove_file(&config).unwrap();
        let other = directory.path().join("other.yaml");
        assert!(register_new_project("example", &other, &registry, "other").is_err());
        assert!(!other.exists());
        register_new_project("example", &config, &registry, "replacement").unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), "replacement");
        assert_eq!(read_registry(&registry).unwrap()["example"], config);
    }

    #[test]
    fn vibe_initialization_declares_a_default_without_provisioning() {
        let root = tempfile::tempdir().unwrap();
        let yaml = initial_config("demo", root.path(), "vibe", None, &Default::default()).unwrap();
        let config: vm_config::config::VmConfig = serde_yaml_ng::from_str(&yaml).unwrap();
        let project = config.project.as_ref().unwrap();
        assert_eq!(project.name.as_deref(), Some("demo"));
        assert_eq!(project.default_environment.as_deref(), Some("dev"));
        assert_eq!(project.workspace_path.as_deref(), Some("/workspace"));
        let dev = &config.environments["dev"];
        assert_eq!(dev.provider.as_str(), "docker");
        assert_eq!(
            dev.image,
            vm_config::config::ImageSpec::String("@vibe-image".into())
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn initialization_respects_user_provider_and_preset_profile() {
        let root = tempfile::tempdir().unwrap();
        let defaults = vm_config::GlobalDefaults {
            provider: Some("podman".into()),
            ..Default::default()
        };
        for (preset, provider, image) in [
            ("vibe", "podman", "@vibe-image"),
            ("base", "podman", "ubuntu:24.04"),
            ("vibe-tart", "tart", "vibe-tart-linux-base"),
        ] {
            let yaml = initial_config("demo", root.path(), preset, None, &defaults).unwrap();
            let config: vm_config::config::VmConfig = serde_yaml_ng::from_str(&yaml).unwrap();
            assert_eq!(config.environments["dev"].provider.as_str(), provider);
            assert_eq!(
                config.environments["dev"].image,
                vm_config::config::ImageSpec::String(image.into())
            );
        }
        assert!(
            initial_config("demo", root.path(), "missing-preset-123", None, &defaults).is_err()
        );
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn project_id_sanitizes_directory_name() {
        assert_eq!(
            project_id(Path::new("/tmp/my project")).unwrap(),
            "my-project"
        );
    }
}
