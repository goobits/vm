use super::*;

#[cfg(feature = "tart")]
pub(in crate::tart) fn record_runtime_receipt(
    instance: &str,
    home: Option<&Path>,
    config: &VmConfig,
) -> Result<()> {
    validate_instance(instance)?;
    let home = home
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(default_home)?;
    let (device, inode) = disk_identity(&home.join("vms").join(instance))?;
    let fingerprint = crate::runtime_fingerprint::runtime_fingerprint(config)?;
    let state_dir = state_dir()?;
    fs::create_dir_all(&state_dir)?;
    vm_core::file_system::set_permissions_mode(&state_dir, 0o700)?;
    let lock = lock(&state_dir)?;
    let path = state_dir.join(STATE_FILE);
    let mut state = read_state_at(&path)?;
    require_runtime_owner(&state, instance, config)?;
    state.runtime_receipts.insert(
        instance.to_string(),
        RuntimeReceipt {
            fingerprint,
            device,
            inode,
        },
    );
    let mut content = serde_json::to_vec_pretty(&state)?;
    content.push(b'\n');
    vm_core::file_system::atomic_write(&path, &content)?;
    vm_core::file_system::set_permissions_mode(&path, 0o600)?;
    FileExt::unlock(&lock)?;
    Ok(())
}

#[cfg(feature = "tart")]
fn require_runtime_owner(state: &StorageState, instance: &str, config: &VmConfig) -> Result<()> {
    if !state.managed.contains(instance) {
        return Err(VmError::validation(
            format!("Tart VM '{instance}' has no managed ownership record"),
            None::<String>,
        ));
    }
    let owner = config.owning_config_path().ok_or_else(|| {
        VmError::validation(
            "Tart runtime has no owning project configuration",
            None::<String>,
        )
    })?;
    if !state.configs.get(instance).is_some_and(|recorded| {
        recorded
            .canonicalize()
            .ok()
            .zip(owner.canonicalize().ok())
            .is_some_and(|(recorded, selected)| recorded == selected)
    }) {
        return Err(VmError::validation(
            format!("Tart VM '{instance}' belongs to a different project configuration"),
            None::<String>,
        ));
    }
    Ok(())
}

#[cfg(feature = "tart")]
pub fn validate_restore_target(instance: &str, config: &VmConfig) -> Result<()> {
    validate_instance(instance)?;
    let state = read_state()?;
    require_runtime_owner(&state, instance, config)?;
    if !state.runtime_receipts.contains_key(instance) {
        return Err(VmError::validation(
            "Tart restore target has no runtime receipt",
            None::<String>,
        ));
    }
    Ok(())
}

#[cfg(feature = "tart")]
pub fn refresh_runtime_identity(
    instance: &str,
    home: Option<&Path>,
    config: &VmConfig,
) -> Result<()> {
    validate_instance(instance)?;
    let home = home
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(default_home)?;
    let identity = disk_identity(&home.join("vms").join(instance))?;
    let directory = state_dir()?;
    let lock = lock(&directory)?;
    let path = directory.join(STATE_FILE);
    let mut state = read_state_at(&path)?;
    refresh_receipt_identity(&mut state, instance, config, identity)?;
    let mut content = serde_json::to_vec_pretty(&state)?;
    content.push(b'\n');
    vm_core::file_system::atomic_write(&path, &content)?;
    vm_core::file_system::set_permissions_mode(&path, 0o600)?;
    FileExt::unlock(&lock)?;
    Ok(())
}

#[cfg(feature = "tart")]
pub(super) fn refresh_receipt_identity(
    state: &mut StorageState,
    instance: &str,
    config: &VmConfig,
    identity: (u64, u64),
) -> Result<()> {
    require_runtime_owner(state, instance, config)?;
    let receipt = state.runtime_receipts.get_mut(instance).ok_or_else(|| {
        VmError::validation("Tart restore target has no runtime receipt", None::<String>)
    })?;
    (receipt.device, receipt.inode) = identity;
    Ok(())
}

#[cfg(feature = "tart")]
pub(in crate::tart) fn runtime_drift(
    instance: &str,
    home: Option<&Path>,
    config: &VmConfig,
) -> Result<Option<String>> {
    validate_instance(instance)?;
    let state = read_state()?;
    let home = home
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(default_home)?;
    detect_runtime_drift(&state, instance, &home, config)
}

#[cfg(feature = "tart")]
pub(super) fn detect_runtime_drift(
    state: &StorageState,
    instance: &str,
    home: &Path,
    config: &VmConfig,
) -> Result<Option<String>> {
    if !state.managed.contains(instance) {
        return Ok(Some("runtime has no managed ownership record".to_string()));
    }
    let Some(owner) = config.owning_config_path() else {
        return Ok(Some(
            "selected project has no owning configuration".to_string(),
        ));
    };
    if !state.configs.get(instance).is_some_and(|recorded| {
        recorded
            .canonicalize()
            .ok()
            .zip(owner.canonicalize().ok())
            .is_some_and(|(recorded, selected)| recorded == selected)
    }) {
        return Ok(Some(
            "runtime belongs to a different project configuration".to_string(),
        ));
    }
    let Some(receipt) = state.runtime_receipts.get(instance) else {
        return Ok(Some("runtime has no configuration fingerprint".to_string()));
    };
    if disk_identity(&home.join("vms").join(instance)).ok() != Some((receipt.device, receipt.inode))
    {
        return Ok(Some(
            "runtime disk identity changed since configuration".to_string(),
        ));
    }
    let expected = crate::runtime_fingerprint::runtime_fingerprint(config)?;
    Ok((receipt.fingerprint != expected)
        .then(|| "runtime configuration differs from selected project".to_string()))
}

#[cfg(all(feature = "tart", unix))]
pub(super) fn disk_identity(path: &Path) -> Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() {
        return Err(VmError::validation(
            format!("Tart VM path is not a directory: {}", path.display()),
            None::<String>,
        ));
    }
    Ok((metadata.dev(), metadata.ino()))
}

#[cfg(all(feature = "tart", not(unix)))]
fn disk_identity(_path: &Path) -> Result<(u64, u64)> {
    Err(VmError::validation(
        "Tart runtime receipts require Unix file identity",
        None::<String>,
    ))
}
