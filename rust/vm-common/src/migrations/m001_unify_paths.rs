use super::Migration;
use std::fs;
use tracing::{debug, info};
use vm_core::error::Result;

pub struct UnifyPaths;

impl Migration for UnifyPaths {
    fn id(&self) -> &'static str {
        "m001_unify_paths"
    }

    fn version(&self) -> &'static str {
        "2.1.0"
    }

    fn description(&self) -> &'static str {
        "Unify all VM data under ~/.vm/"
    }

    fn run(&self) -> Result<()> {
        migrate_global_config()?;
        rename_state_files()?;
        Ok(())
    }
}

fn migrate_global_config() -> Result<()> {
    let old_path = crate::user_paths::user_config_dir()?.join("global.yaml");
    let new_path = crate::user_paths::vm_state_dir()?.join("config.yaml");

    if old_path.exists() && !new_path.exists() {
        info!("Migrating global config: {:?} -> {:?}", old_path, new_path);
        fs::create_dir_all(new_path.parent().unwrap())?;
        fs::copy(&old_path, &new_path)?;
        debug!("Global config migration completed");
    } else if new_path.exists() {
        debug!("Global config already at new location");
    } else {
        debug!("No global config found to migrate");
    }

    Ok(())
}

fn rename_state_files() -> Result<()> {
    let vm_state_dir = crate::user_paths::vm_state_dir()?;
    let renames = [
        ("service_state.json", "services.json"),
        ("port-registry.json", "ports.json"),
        ("temp-vm.state", "temp-vms.json"),
    ];

    for (old_name, new_name) in renames {
        let old_path = vm_state_dir.join(old_name);
        let new_path = vm_state_dir.join(new_name);

        if old_path.exists() && !new_path.exists() {
            info!("Renaming state file: {} -> {}", old_name, new_name);
            fs::rename(&old_path, &new_path)?;
        } else if new_path.exists() {
            debug!("State file {} already renamed", new_name);
        } else {
            debug!("No state file {} found to rename", old_name);
        }
    }

    Ok(())
}
