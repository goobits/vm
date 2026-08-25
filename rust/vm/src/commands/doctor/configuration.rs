pub(super) fn check_directory() -> Result<(), String> {
    let directory = vm_core::user_paths::user_config_dir()
        .map_err(|_| "Cannot determine config directory".to_string())?;
    if directory.exists() {
        Ok(())
    } else {
        Err(format!(
            "Config directory doesn't exist: {}",
            directory.display()
        ))
    }
}

pub(super) fn create_directory() -> bool {
    vm_core::user_paths::user_config_dir()
        .is_ok_and(|directory| std::fs::create_dir_all(directory).is_ok())
}
