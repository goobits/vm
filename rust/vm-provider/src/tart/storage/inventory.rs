use super::*;

/// A recorded VM disk in Tart's local store. Only exact managed names are
/// inventoried; arbitrary VMs under TART_HOME are never claimed.
#[cfg(feature = "tart")]
#[derive(Debug, Clone)]
pub struct TartStorageEntry {
    pub id: String,
    pub owner: String,
    pub reason: Option<String>,
}

#[cfg(feature = "tart")]
pub fn storage_inventory() -> Result<Vec<TartStorageEntry>> {
    let state = read_state()?;
    let mut entries = Vec::new();
    let mut listings = BTreeMap::new();
    for instance in state.clone().managed_instances() {
        validate_instance(&instance)?;
        let home = state
            .instances
            .get(&instance)
            .cloned()
            .map(Ok)
            .unwrap_or_else(default_home)?;
        let vm_dir = home.join("vms").join(&instance);
        if !vm_dir.exists() {
            continue;
        }
        let reason = if !is_plain_directory(&home)
            || !is_plain_directory(&home.join("vms"))
            || !is_plain_directory(&vm_dir)
        {
            Some("storage path contains a symlink or is not a directory".to_string())
        } else if state
            .configs
            .get(&instance)
            .is_some_and(|path| path.exists())
        {
            Some("registered project configuration still exists".to_string())
        } else {
            listings
                .entry(home.clone())
                .or_insert_with(|| TartListing::load(&home))
                .reason(&instance)
        };
        let owner = state.configs.get(&instance).map_or_else(
            || format!("managed Tart VM {instance}"),
            |config| {
                format!(
                    "managed Tart VM {instance}, project config {}",
                    config.display()
                )
            },
        );
        entries.push(TartStorageEntry {
            id: format!("tart:vm:{instance}"),
            owner,
            reason,
        });
    }
    Ok(entries)
}

#[cfg(feature = "tart")]
pub fn remove_storage(id: &str) -> Result<()> {
    let instance = id
        .strip_prefix("tart:vm:")
        .ok_or_else(|| VmError::validation("Invalid Tart storage ID", None::<String>))?;
    validate_instance(instance)?;
    let entry = storage_inventory()?
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| {
            VmError::validation(
                "Tart storage resource is no longer in the managed inventory",
                None::<String>,
            )
        })?;
    if let Some(reason) = entry.reason {
        return Err(VmError::validation(
            format!("Cannot remove '{id}': {reason}"),
            None::<String>,
        ));
    }
    let state = read_state()?;
    let home = state
        .instances
        .get(instance)
        .cloned()
        .map(Ok)
        .unwrap_or_else(default_home)?;
    let output = Command::new("tart")
        .args(["delete", instance])
        .env("TART_HOME", &home)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| VmError::general(error, "Failed to execute Tart storage removal"))?;
    if !output.status.success() {
        return Err(VmError::validation(
            format!(
                "Tart did not remove '{instance}': {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            None::<String>,
        ));
    }
    forget_instance(instance)
}

#[cfg(feature = "tart")]
pub(super) fn is_plain_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_dir())
}

#[cfg(feature = "tart")]
enum TartListing {
    Entries(Vec<serde_json::Value>),
    Unavailable(&'static str),
}

#[cfg(feature = "tart")]
impl TartListing {
    fn load(home: &Path) -> Self {
        let output = Command::new("tart")
            .args(["list", "--format", "json"])
            .env("TART_HOME", home)
            .stdin(Stdio::null())
            .output();
        let Ok(output) = output else {
            return Self::Unavailable("Tart executable is unavailable");
        };
        if !output.status.success() {
            return Self::Unavailable("Tart inventory failed");
        }
        match serde_json::from_slice(&output.stdout) {
            Ok(entries) => Self::Entries(entries),
            Err(_) => Self::Unavailable("Tart inventory is invalid"),
        }
    }

    fn reason(&self, instance: &str) -> Option<String> {
        let entries = match self {
            Self::Entries(entries) => entries,
            Self::Unavailable(reason) => return Some((*reason).to_string()),
        };
        match entries
            .iter()
            .find(|entry| entry["Name"].as_str() == Some(instance))
        {
            Some(entry)
                if entry["Source"] == "local"
                    && entry["State"]
                        .as_str()
                        .is_some_and(|state| state.eq_ignore_ascii_case("stopped")) =>
            {
                None
            }
            Some(_) => Some("VM is active or not a local Tart VM".to_string()),
            None => Some("VM is not visible in Tart inventory".to_string()),
        }
    }
}
