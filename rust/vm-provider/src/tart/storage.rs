use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
#[cfg(feature = "tart")]
use std::process::Command;
#[cfg(feature = "tart")]
use std::process::Stdio;
use vm_config::config::VmConfig;
use vm_core::error::{Result, VmError};

#[cfg(feature = "tart")]
mod inventory;
#[cfg(feature = "tart")]
mod receipt;
#[cfg(feature = "tart")]
pub use inventory::{remove_storage, storage_inventory, TartStorageEntry};
#[cfg(feature = "tart")]
pub(super) use receipt::{record_runtime_receipt, runtime_drift};
#[cfg(feature = "tart")]
pub use receipt::{refresh_runtime_identity, snapshot_fingerprint, validate_restore_target};

const STATE_DIRECTORY: &str = "tart";
const STATE_FILE: &str = "instances.json";
const LOCK_FILE: &str = "instances.lock";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StorageState {
    managed: BTreeSet<String>,
    instances: BTreeMap<String, PathBuf>,
    configs: BTreeMap<String, PathBuf>,
    #[serde(default)]
    runtime_receipts: BTreeMap<String, RuntimeReceipt>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RuntimeReceipt {
    fingerprint: String,
    device: u64,
    inode: u64,
}

impl StorageState {
    fn managed_instances(mut self) -> BTreeSet<String> {
        self.managed.extend(self.configs.into_keys());
        self.managed
    }
}

pub(super) fn configured_home(config: Option<&VmConfig>) -> Option<PathBuf> {
    config
        .and_then(|config| config.tart.as_ref())
        .and_then(|tart| tart.storage_path.as_deref())
        .filter(|path| !path.trim().is_empty())
        .map(expand_home)
        .or_else(|| {
            std::env::var_os("TART_HOME")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
}

pub(super) fn resolve_project_home(config: &VmConfig, project: &str) -> Result<Option<PathBuf>> {
    if let Some(home) = configured_home(Some(config)) {
        return Ok(Some(home));
    }

    let state = read_state()?;
    recorded_project_home(&state, config.owning_config_path(), project)
}

fn recorded_project_home(
    state: &StorageState,
    owner: Option<&Path>,
    project: &str,
) -> Result<Option<PathBuf>> {
    let mut homes = state
        .instances
        .iter()
        .filter(|(instance, _)| belongs_to_project(instance, project))
        .filter(|(instance, _)| {
            state
                .configs
                .get(*instance)
                .zip(owner)
                .is_some_and(|(recorded, selected)| {
                    recorded
                        .canonicalize()
                        .ok()
                        .zip(selected.canonicalize().ok())
                        .is_some_and(|(recorded, selected)| recorded == selected)
                })
        })
        .map(|(_, home)| home.clone())
        .collect::<BTreeSet<_>>();
    if homes.len() > 1 {
        return Err(VmError::validation(
            format!("Project '{project}' has Tart instances in multiple storage homes"),
            Some("Set tart.storage_path explicitly in the project configuration"),
        ));
    }
    Ok(homes.pop_first())
}

/// Resolve the same Tart home used for project runtime operations.
pub fn project_home(config: &VmConfig, project: &str) -> Result<Option<PathBuf>> {
    resolve_project_home(config, project)
}

pub(super) fn remember_instance(
    instance: &str,
    home: Option<&Path>,
    config: Option<&Path>,
) -> Result<()> {
    validate_instance(instance)?;
    let state_dir = state_dir()?;
    fs::create_dir_all(&state_dir)?;
    vm_core::file_system::set_permissions_mode(&state_dir, 0o700)?;
    let lock = lock(&state_dir)?;
    let path = state_dir.join(STATE_FILE);
    let mut state = read_state_at(&path)?;
    state.managed.insert(instance.to_string());
    if let Some(home) = home {
        state
            .instances
            .insert(instance.to_string(), home.to_path_buf());
    }
    if let Some(config) = config {
        state
            .configs
            .insert(instance.to_string(), config.to_path_buf());
    }
    let mut content = serde_json::to_vec_pretty(&state)?;
    content.push(b'\n');
    vm_core::file_system::atomic_write(&path, &content)?;
    vm_core::file_system::set_permissions_mode(&path, 0o600)?;
    FileExt::unlock(&lock)?;
    Ok(())
}

#[cfg(feature = "tart")]
pub(super) fn forget_instance(instance: &str) -> Result<()> {
    validate_instance(instance)?;
    let state_dir = state_dir()?;
    if !state_dir.exists() {
        return Ok(());
    }
    let lock = lock(&state_dir)?;
    let path = state_dir.join(STATE_FILE);
    let mut state = read_state_at(&path)?;
    let removed_managed = state.managed.remove(instance);
    let removed_home = state.instances.remove(instance).is_some();
    let removed_config = state.configs.remove(instance).is_some();
    let removed_receipt = state.runtime_receipts.remove(instance).is_some();
    let changed = removed_managed || removed_home || removed_config || removed_receipt;
    if changed {
        let mut content = serde_json::to_vec_pretty(&state)?;
        content.push(b'\n');
        vm_core::file_system::atomic_write(&path, &content)?;
        vm_core::file_system::set_permissions_mode(&path, 0o600)?;
    }
    FileExt::unlock(&lock)?;
    Ok(())
}

pub(super) fn instance_config_path(instance: &str) -> Result<Option<PathBuf>> {
    validate_instance(instance)?;
    Ok(read_state()?.configs.get(instance).cloned())
}

pub(super) fn managed_instances() -> Result<BTreeSet<String>> {
    Ok(read_state()?.managed_instances())
}

#[cfg(feature = "tart")]
fn default_home() -> Result<PathBuf> {
    Ok(vm_core::user_paths::home_dir()?.join(".tart"))
}

fn read_state() -> Result<StorageState> {
    read_state_at(&state_dir()?.join(STATE_FILE))
}

fn read_state_at(path: &Path) -> Result<StorageState> {
    match fs::read(path) {
        Ok(content) => serde_json::from_slice(&content).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StorageState::default()),
        Err(error) => Err(error.into()),
    }
}

fn state_dir() -> Result<PathBuf> {
    Ok(vm_core::user_paths::vm_state_dir()?.join(STATE_DIRECTORY))
}

fn lock(state_dir: &Path) -> Result<File> {
    let path = state_dir.join(LOCK_FILE);
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)?;
    vm_core::file_system::set_permissions_mode(&path, 0o600)?;
    file.lock_exclusive()?;
    Ok(file)
}

fn expand_home(path: &str) -> PathBuf {
    if path == "~" {
        return vm_core::user_paths::home_dir().unwrap_or_else(|_| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = vm_core::user_paths::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn belongs_to_project(instance: &str, project: &str) -> bool {
    instance == project
        || instance
            .strip_prefix(project)
            .is_some_and(|suffix| suffix.starts_with('-'))
}

fn validate_instance(instance: &str) -> Result<()> {
    if instance.is_empty()
        || !instance.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        || instance == "."
        || instance == ".."
    {
        return Err(VmError::validation(
            format!("Invalid Tart instance name '{instance}'"),
            None::<String>,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{belongs_to_project, recorded_project_home, StorageState};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn project_matching_does_not_use_ambiguous_prefixes() {
        assert!(belongs_to_project("vm", "vm"));
        assert!(belongs_to_project("vm-mac", "vm"));
        assert!(!belongs_to_project("vmmac", "vm"));
    }

    #[test]
    fn incomplete_storage_state_is_rejected() {
        assert!(
            serde_json::from_str::<StorageState>(r#"{"instances":{"demo":"/tmp/tart"}}"#).is_err()
        );
    }

    #[test]
    fn managed_inventory_requires_creation_or_config_ownership() {
        let state = StorageState {
            managed: BTreeSet::from(["created".into()]),
            instances: BTreeMap::from([("recovered-home-only".into(), "/tmp/tart".into())]),
            configs: BTreeMap::from([(
                "owned-before-managed-marker".into(),
                "/work/vm.yaml".into(),
            )]),
            runtime_receipts: BTreeMap::new(),
        };

        assert_eq!(
            state.managed_instances(),
            BTreeSet::from(["created".into(), "owned-before-managed-marker".into()])
        );
    }

    #[test]
    fn storage_home_resolution_requires_exact_config_owner() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.yaml");
        let second = root.path().join("second.yaml");
        std::fs::write(&first, "project: first").unwrap();
        std::fs::write(&second, "project: second").unwrap();
        let state = StorageState {
            managed: BTreeSet::new(),
            instances: BTreeMap::from([
                ("demo-dev".into(), root.path().join("first-home")),
                ("demo-test".into(), root.path().join("second-home")),
            ]),
            configs: BTreeMap::from([
                ("demo-dev".into(), first.clone()),
                ("demo-test".into(), second.clone()),
            ]),
            runtime_receipts: BTreeMap::new(),
        };
        assert_eq!(
            recorded_project_home(&state, Some(&first), "demo").unwrap(),
            Some(root.path().join("first-home"))
        );
        assert_eq!(
            recorded_project_home(&state, Some(&second), "demo").unwrap(),
            Some(root.path().join("second-home"))
        );
        assert_eq!(recorded_project_home(&state, None, "demo").unwrap(), None);
    }

    #[test]
    #[cfg(all(feature = "tart", unix))]
    fn storage_inventory_rejects_symlinked_vm_directory() {
        let root = tempfile::tempdir().unwrap();
        let vms = root.path().join("vms");
        std::fs::create_dir(&vms).unwrap();
        let outside = root.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, vms.join("demo")).unwrap();
        assert!(!super::inventory::is_plain_directory(&vms.join("demo")));
        assert!(super::inventory::is_plain_directory(&vms));
    }

    #[test]
    #[cfg(all(feature = "tart", unix))]
    fn runtime_receipt_detects_configuration_and_disk_replacement() {
        let root = tempfile::tempdir().unwrap();
        let owner = root.path().join("vm.yaml");
        std::fs::write(&owner, "project: demo").unwrap();
        let home = root.path().join("tart");
        let vm = home.join("vms/demo-dev");
        std::fs::create_dir_all(&vm).unwrap();
        let mut config = vm_config::config::VmConfig {
            source_path: Some(owner.clone()),
            ..Default::default()
        };
        let (device, inode) = super::receipt::disk_identity(&vm).unwrap();
        let mut state = StorageState {
            managed: BTreeSet::from(["demo-dev".into()]),
            instances: BTreeMap::from([("demo-dev".into(), home.clone())]),
            configs: BTreeMap::from([("demo-dev".into(), owner)]),
            runtime_receipts: BTreeMap::from([(
                "demo-dev".into(),
                super::RuntimeReceipt {
                    fingerprint: crate::runtime_fingerprint::runtime_fingerprint(&config).unwrap(),
                    device,
                    inode,
                },
            )]),
        };
        assert_eq!(
            super::receipt::detect_runtime_drift(&state, "demo-dev", &home, &config).unwrap(),
            None
        );
        config.environment.insert("MODE".into(), "changed".into());
        // Capture the runtime's recorded configuration, not newly edited project settings.
        assert_eq!(
            super::receipt::captured_fingerprint(&state, "demo-dev", &home, &config).unwrap(),
            state.runtime_receipts["demo-dev"].fingerprint
        );
        assert!(
            super::receipt::detect_runtime_drift(&state, "demo-dev", &home, &config)
                .unwrap()
                .is_some()
        );
        config.environment.clear();
        std::fs::rename(&vm, home.join("vms/previous")).unwrap();
        std::fs::create_dir(&vm).unwrap();
        assert!(
            super::receipt::captured_fingerprint(&state, "demo-dev", &home, &config)
                .unwrap_err()
                .to_string()
                .contains("disk identity")
        );
        assert!(
            super::receipt::detect_runtime_drift(&state, "demo-dev", &home, &config)
                .unwrap()
                .unwrap()
                .contains("disk identity")
        );
        let identity = super::receipt::disk_identity(&vm).unwrap();
        let captured = state.runtime_receipts["demo-dev"].fingerprint.clone();
        super::receipt::refresh_receipt_identity(
            &mut state, "demo-dev", &config, identity, &captured,
        )
        .unwrap();
        assert_eq!(
            super::receipt::detect_runtime_drift(&state, "demo-dev", &home, &config).unwrap(),
            None
        );
        config.environment.insert("MODE".into(), "changed".into());
        state
            .runtime_receipts
            .get_mut("demo-dev")
            .unwrap()
            .fingerprint = crate::runtime_fingerprint::runtime_fingerprint(&config).unwrap();
        assert_eq!(
            super::receipt::detect_runtime_drift(&state, "demo-dev", &home, &config).unwrap(),
            None
        );
        super::receipt::refresh_receipt_identity(
            &mut state, "demo-dev", &config, identity, &captured,
        )
        .unwrap();
        assert!(
            super::receipt::detect_runtime_drift(&state, "demo-dev", &home, &config)
                .unwrap()
                .unwrap()
                .contains("configuration differs")
        );
        let other = root.path().join("other.yaml");
        std::fs::write(&other, "project: demo").unwrap();
        config.source_path = Some(other);
        assert!(super::receipt::refresh_receipt_identity(
            &mut state,
            "demo-dev",
            &config,
            (0, 0),
            &captured
        )
        .unwrap_err()
        .to_string()
        .contains("different project"));
        assert_eq!(
            (
                state.runtime_receipts["demo-dev"].device,
                state.runtime_receipts["demo-dev"].inode
            ),
            identity
        );
    }
}
