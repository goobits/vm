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

pub(super) fn init(path: Option<PathBuf>) -> VmResult<()> {
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
    register_new_project(&id, &config, &registry)?;
    println!("Initialized project {id} at {}", config.display());
    Ok(())
}

fn register_new_project(id: &str, config: &Path, registry: &Path) -> VmResult<()> {
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
    if let Some(existing) = entries.get(id) {
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
    let contents = format!("version: '2.0'\nproject:\n  name: {id}\nprovider: docker\n");
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
        register_new_project("example", &config, &registry).unwrap();
        assert!(fs::read_to_string(&config)
            .unwrap()
            .contains("name: example"));
        assert_eq!(read_registry(&registry).unwrap()["example"], config);
        assert!(register_new_project("example", &config, &registry).is_err());
    }

    #[test]
    fn project_id_sanitizes_directory_name() {
        assert_eq!(
            project_id(Path::new("/tmp/my project")).unwrap(),
            "my-project"
        );
    }
}
