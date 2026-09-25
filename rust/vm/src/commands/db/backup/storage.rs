//! Backup files, retention, and namespaced storage.

use std::path::{Component, Path, PathBuf};
use vm_config::GlobalConfig;

use super::{confirm_destructive, DbRoute};
use crate::error::{VmError, VmResult};

/// Get the base directory for backups
pub(super) fn get_backup_dir(route: &DbRoute) -> VmResult<PathBuf> {
    let global_config = GlobalConfig::load()?;

    // Expand tilde in configured backup path
    let expanded_path = shellexpand::tilde(&global_config.backups.path);
    let backup_dir = PathBuf::from(expanded_path.as_ref())
        .join("postgres")
        .join(&route.backup_namespace);

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| VmError::filesystem(e, backup_dir.to_string_lossy(), "create_dir_all"))?;
    Ok(backup_dir)
}

pub(crate) fn validate_backup_component(name: &str) -> VmResult<()> {
    let mut components = Path::new(name).components();
    let is_single_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();

    if !is_single_component {
        return Err(VmError::validation(
            format!("Invalid backup name '{name}'"),
            Some("Use a filename without directories or traversal components".to_string()),
        ));
    }

    Ok(())
}

fn belongs_to_database(file_name: &str, database: &str) -> bool {
    file_name.starts_with(&format!("{database}_")) && file_name.ends_with(".dump")
}

/// Get the number of backups for a specific database
pub async fn count_backups(route: &DbRoute, db_name: &str) -> VmResult<usize> {
    let backup_dir = get_backup_dir(route)?;

    if !backup_dir.exists() {
        return Ok(0);
    }

    let mut read_dir = tokio::fs::read_dir(&backup_dir)
        .await
        .map_err(|e| VmError::filesystem(e, backup_dir.to_string_lossy(), "read_dir"))?;

    let mut count = 0;
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| VmError::general(e, "Failed to read backup directory entries"))?
    {
        if entry.file_type().await?.is_file()
            && belongs_to_database(&entry.file_name().to_string_lossy(), db_name)
        {
            count += 1;
        }
    }

    Ok(count)
}

/// Get the backup directory path as a string
pub fn get_backup_path(route: &DbRoute) -> VmResult<String> {
    Ok(get_backup_dir(route)?.to_string_lossy().to_string())
}

/// List exact retained backup file names, optionally filtering by database prefix.
pub fn list_backups(route: &DbRoute, database: Option<&str>) -> VmResult<Vec<String>> {
    if let Some(name) = database {
        validate_backup_component(name)?;
    }
    let mut backups = Vec::new();
    for entry in std::fs::read_dir(get_backup_dir(route)?)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if database.map_or(file_name.ends_with(".dump"), |name| {
            belongs_to_database(&file_name, name)
        }) {
            backups.push(file_name);
        }
    }
    backups.sort();
    Ok(backups)
}

/// Remove one retained backup by its exact file name.
pub fn remove_backup(route: &DbRoute, name: &str, yes: bool) -> VmResult<()> {
    validate_backup_component(name)?;
    if !name.ends_with(".dump") {
        return Err(VmError::validation(
            "Backup name must end in .dump",
            None::<String>,
        ));
    }
    if !belongs_to_database(name, &route.database) {
        return Err(VmError::validation(
            format!(
                "Backup '{name}' does not belong to database '{}'",
                route.database
            ),
            None::<String>,
        ));
    }
    let path = get_backup_dir(route)?.join(name);
    if !path.is_file() {
        return Err(VmError::validation(
            format!("Backup '{name}' not found"),
            None::<String>,
        ));
    }
    if !confirm_destructive(&format!("Permanently remove backup '{name}'?"), yes)? {
        return Ok(());
    }
    std::fs::remove_file(path)?;
    vm_core::vm_success!("Removed backup '{name}'");
    Ok(())
}

/// Clean up old backups, keeping only the most recent `retention_count`
pub(super) async fn clean_old_backups(
    route: &DbRoute,
    db_name: &str,
    retention_count: u32,
) -> VmResult<()> {
    let backup_dir = get_backup_dir(route)?;
    let mut read_dir = tokio::fs::read_dir(&backup_dir)
        .await
        .map_err(|e| VmError::filesystem(e, backup_dir.to_string_lossy(), "read_dir"))?;

    let mut entries_with_meta: Vec<(tokio::fs::DirEntry, std::time::SystemTime)> = Vec::new();
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| VmError::general(e, "Failed to read backup directory entries"))?
    {
        let metadata = entry
            .metadata()
            .await
            .map_err(|e| VmError::general(e, "Failed to get metadata"))?;
        if metadata.is_file() {
            let modified = metadata
                .modified()
                .map_err(|e| VmError::general(e, "Failed to get modification time"))?;
            entries_with_meta.push((entry, modified));
        }
    }
    let mut backups = entries_with_meta;

    // Filter for backups of the specified database and sort by modification time (newest first)
    backups.sort_by_key(|(_, created)| *created);
    backups.reverse();

    let db_backups: Vec<_> = backups
        .into_iter()
        .filter(|(entry, _)| belongs_to_database(&entry.file_name().to_string_lossy(), db_name))
        .collect();

    if db_backups.len() > retention_count as usize {
        for (backup_to_delete, _) in db_backups.iter().skip(retention_count as usize) {
            vm_core::vm_println!("Deleting old backup: {:?}", backup_to_delete.path());
            tokio::fs::remove_file(backup_to_delete.path())
                .await
                .map_err(|e| {
                    VmError::filesystem(e, backup_to_delete.path().to_string_lossy(), "remove_file")
                })?;
        }
    }

    Ok(())
}
