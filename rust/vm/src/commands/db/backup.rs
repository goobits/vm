//! DB backup and restore logic
use crate::error::{VmError, VmResult};
use chrono::Local;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use vm_config::GlobalConfig;

/// Get the base directory for backups
fn get_backup_dir() -> VmResult<PathBuf> {
    let global_config = GlobalConfig::load()?;

    // Expand tilde in configured backup path
    let expanded_path = shellexpand::tilde(&global_config.backups.path);
    let backup_dir = PathBuf::from(expanded_path.as_ref()).join("postgres");

    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| VmError::filesystem(e, backup_dir.to_string_lossy(), "create_dir_all"))?;
    Ok(backup_dir)
}

/// Execute a command in the postgres docker container
async fn execute_docker_command(args: &[&str], input: Option<&[u8]>) -> VmResult<Vec<u8>> {
    let global_config = GlobalConfig::load()?;
    let provider = global_config.container_provider();
    let mut cmd = tokio::process::Command::new(provider.as_str());
    cmd.arg("exec").arg("-i").arg("vm-postgres-global");
    cmd.args(args);

    if input.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| VmError::general(e, "Failed to spawn docker command"))?;

    if let (Some(input_data), Some(mut stdin)) = (input, child.stdin.take()) {
        use tokio::io::AsyncWriteExt;
        if let Err(e) = stdin.write_all(input_data).await {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(VmError::general(
                e,
                "Failed to write to docker command stdin",
            ));
        }
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| VmError::general(e, "Failed to wait for docker command"))?;

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(VmError::general(
            std::io::Error::new(std::io::ErrorKind::Other, "Docker command failed"),
            String::from_utf8_lossy(&output.stderr),
        ))
    }
}

fn validate_backup_component(name: &str) -> VmResult<()> {
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

fn quote_pg_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_pg_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

async fn execute_admin_sql(query: &str) -> VmResult<Vec<u8>> {
    execute_docker_command(
        &[
            "psql",
            "-U",
            "postgres",
            "-d",
            "postgres",
            "-v",
            "ON_ERROR_STOP=1",
            "-tA",
            "-c",
            query,
        ],
        None,
    )
    .await
}

async fn database_exists(db_name: &str) -> VmResult<bool> {
    let query = format!(
        "SELECT 1 FROM pg_database WHERE datname = {};",
        quote_pg_literal(db_name)
    );
    let output = execute_admin_sql(&query).await?;
    Ok(String::from_utf8_lossy(&output).trim() == "1")
}

async fn disconnect_database(db_name: &str) -> VmResult<()> {
    let query = format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = {} AND pid <> pg_backend_pid();",
        quote_pg_literal(db_name)
    );
    execute_admin_sql(&query).await?;
    Ok(())
}

async fn create_database(db_name: &str) -> VmResult<()> {
    execute_docker_command(&["createdb", "-U", "postgres", "--", db_name], None).await?;
    Ok(())
}

async fn drop_database(db_name: &str) -> VmResult<()> {
    disconnect_database(db_name).await?;
    execute_docker_command(
        &["dropdb", "-U", "postgres", "--if-exists", "--", db_name],
        None,
    )
    .await?;
    Ok(())
}

async fn rename_database(from: &str, to: &str) -> VmResult<()> {
    let query = format!(
        "ALTER DATABASE {} RENAME TO {};",
        quote_pg_identifier(from),
        quote_pg_identifier(to)
    );
    execute_admin_sql(&query).await?;
    Ok(())
}

async fn replace_database(staging_name: &str, db_name: &str, previous_name: &str) -> VmResult<()> {
    let had_previous = match database_exists(db_name).await {
        Ok(exists) => exists,
        Err(error) => {
            let _ = drop_database(staging_name).await;
            return Err(error);
        }
    };

    if had_previous {
        if let Err(error) = disconnect_database(db_name).await {
            let _ = drop_database(staging_name).await;
            return Err(error);
        }
        if let Err(error) = rename_database(db_name, previous_name).await {
            let _ = drop_database(staging_name).await;
            return Err(error);
        }
    }

    if let Err(error) = rename_database(staging_name, db_name).await {
        if had_previous {
            if let Err(recovery_error) = rename_database(previous_name, db_name).await {
                return Err(VmError::general(
                    recovery_error,
                    format!(
                        "Failed to promote replacement database and to recover original database '{db_name}'"
                    ),
                ));
            }
        }
        let _ = drop_database(staging_name).await;
        return Err(error);
    }

    if had_previous {
        if let Err(error) = drop_database(previous_name).await {
            vm_core::vm_warning!(
                "Database was replaced, but the previous copy '{}' could not be removed: {}",
                previous_name,
                error
            );
        }
    }

    Ok(())
}

/// Backup a database
pub async fn backup_db(
    db_name: &str,
    backup_name: Option<&str>,
    retention_count: u32,
) -> VmResult<()> {
    validate_backup_component(backup_name.unwrap_or(db_name))?;
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let backup_file_name = match backup_name {
        Some(name) => format!("{name}_{timestamp}.dump"),
        None => format!("{db_name}_{timestamp}.dump"),
    };
    let backup_path = get_backup_dir()?.join(&backup_file_name);

    let output = execute_docker_command(
        &[
            "pg_dump", "-U", "postgres", "-d", db_name, "-F", "c", // Custom format, compressed
        ],
        None,
    )
    .await?;

    tokio::fs::write(&backup_path, output)
        .await
        .map_err(|e| VmError::filesystem(e, backup_path.to_string_lossy(), "write"))?;

    vm_core::vm_success!("Database '{}' backed up to {:?}", db_name, backup_path);

    if retention_count > 0 {
        clean_old_backups(db_name, retention_count).await?;
    }

    Ok(())
}

/// Restore a database
pub async fn restore_db(backup_name: &str, db_name: &str) -> VmResult<()> {
    validate_backup_component(backup_name)?;
    let backup_path = get_backup_dir()?.join(backup_name);
    if !backup_path.exists() {
        return Err(VmError::validation(
            "Backup file not found",
            Some(format!("Backup file not found at: {backup_path:?}")),
        ));
    }

    let backup_data = tokio::fs::read(&backup_path)
        .await
        .map_err(|e| VmError::filesystem(e, backup_path.to_string_lossy(), "read"))?;

    // Validate the archive before touching any database.
    execute_docker_command(&["pg_restore", "--list"], Some(&backup_data)).await?;

    // Restore into a staging database so the current database stays intact if
    // validation or restoration fails.
    let operation_id = Uuid::new_v4().simple().to_string();
    let staging_name = format!("vm_restore_{operation_id}");
    let previous_name = format!("vm_previous_{operation_id}");
    create_database(&staging_name).await?;

    let restore_result = execute_docker_command(
        &[
            "pg_restore",
            "-U",
            "postgres",
            "-d",
            &staging_name,
            "--exit-on-error",
        ],
        Some(&backup_data),
    )
    .await;
    if let Err(error) = restore_result {
        let _ = drop_database(&staging_name).await;
        return Err(error);
    }

    replace_database(&staging_name, db_name, &previous_name).await?;

    vm_core::vm_success!("Database '{}' restored from '{}'", db_name, backup_name);
    Ok(())
}

/// Export a database to a SQL file
pub async fn export_db(db_name: &str, file: &Path) -> VmResult<()> {
    let output = execute_docker_command(
        &["pg_dump", "-U", "postgres", "-d", db_name, "--clean"],
        None,
    )
    .await?;

    tokio::fs::write(file, output)
        .await
        .map_err(|e| VmError::filesystem(e, file.to_string_lossy(), "write"))?;

    vm_core::vm_success!("Database '{}' exported to {:?}", db_name, file);
    Ok(())
}

/// Import a database from a SQL file
pub async fn import_db(db_name: &str, file: &Path) -> VmResult<()> {
    if !file.exists() {
        return Err(VmError::validation(
            "Import file not found",
            Some(format!("Import file not found at: {file:?}")),
        ));
    }

    let sql_data = tokio::fs::read(file)
        .await
        .map_err(|e| VmError::filesystem(e, file.to_string_lossy(), "read"))?;

    execute_docker_command(&["psql", "-U", "postgres", "-d", db_name], Some(&sql_data)).await?;

    vm_core::vm_success!("Database '{}' imported from {:?}", db_name, file);
    Ok(())
}

/// Reset a database
pub async fn reset_db(db_name: &str, force: bool) -> VmResult<()> {
    if !force {
        vm_core::vm_warning!("This will permanently delete all data in the '{db_name}' database");
        if !vm_core::prompts::confirm_select("Continue?", false)? {
            vm_core::vm_println!("Database reset cancelled.");
            return Ok(());
        }
    }

    let operation_id = Uuid::new_v4().simple().to_string();
    let staging_name = format!("vm_reset_{operation_id}");
    let previous_name = format!("vm_previous_{operation_id}");
    create_database(&staging_name).await?;
    replace_database(&staging_name, db_name, &previous_name).await?;

    vm_core::vm_success!("Database '{}' has been reset.", db_name);
    Ok(())
}

/// Get the number of backups for a specific database
pub async fn count_backups(db_name: &str) -> VmResult<usize> {
    let backup_dir = get_backup_dir()?;

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
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(&format!("{db_name}_"))
        {
            count += 1;
        }
    }

    Ok(count)
}

/// Get the backup directory path as a string
pub fn get_backup_path() -> VmResult<String> {
    Ok(get_backup_dir()?.to_string_lossy().to_string())
}

/// Clean up old backups, keeping only the most recent `retention_count`
async fn clean_old_backups(db_name: &str, retention_count: u32) -> VmResult<()> {
    let backup_dir = get_backup_dir()?;
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
            let created = metadata
                .created()
                .map_err(|e| VmError::general(e, "Failed to get creation time"))?;
            entries_with_meta.push((entry, created));
        }
    }
    let mut backups = entries_with_meta;

    // Filter for backups of the specified database and sort by creation time (newest first)
    backups.sort_by_key(|(_, created)| *created);
    backups.reverse();

    let db_backups: Vec<_> = backups
        .into_iter()
        .filter(|(entry, _)| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{db_name}_"))
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_values_are_quoted_as_one_identifier_or_literal() {
        assert_eq!(
            quote_pg_identifier("db\"; DROP DATABASE postgres; --"),
            "\"db\"\"; DROP DATABASE postgres; --\""
        );
        assert_eq!(
            quote_pg_literal("db'; SELECT 1; --"),
            "'db''; SELECT 1; --'"
        );
    }

    #[test]
    fn backup_names_reject_paths() {
        assert!(validate_backup_component("database.dump").is_ok());
        for name in ["", ".", "..", "../database.dump", "/tmp/database.dump"] {
            assert!(validate_backup_component(name).is_err());
        }
    }
}
