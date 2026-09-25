//! PostgreSQL backup, restore, import, and export operations.

use crate::error::{VmError, VmResult};
use chrono::Local;
use std::path::Path;
use uuid::Uuid;

use super::route::DbRoute;

mod postgres;
mod storage;

pub(super) use postgres::quote_pg_literal;
use postgres::{create_database, drop_database, execute_docker_command, replace_database};
pub(super) use storage::validate_backup_component;
use storage::{clean_old_backups, get_backup_dir};
pub use storage::{count_backups, get_backup_path, list_backups, remove_backup};

/// Backup a database
pub async fn backup_db(
    route: &DbRoute,
    db_name: &str,
    backup_name: Option<&str>,
    retention_count: u32,
) -> VmResult<()> {
    validate_backup_component(db_name)?;
    if let Some(name) = backup_name {
        validate_backup_component(name)?;
    }
    let timestamp = Local::now().format("%Y%m%d_%H%M%S_%6f");
    let backup_file_name = match backup_name {
        Some(name) => format!("{db_name}_{name}_{timestamp}.dump"),
        None => format!("{db_name}_{timestamp}.dump"),
    };
    let backup_path = get_backup_dir(route)?.join(&backup_file_name);

    let output = execute_docker_command(
        route,
        &[
            "pg_dump", "-U", "postgres", "-d", db_name, "-F", "c", // Custom format, compressed
        ],
        None,
    )
    .await?;

    vm_core::file_system::atomic_write(&backup_path, &output)
        .map_err(|e| VmError::filesystem(e, backup_path.to_string_lossy(), "write"))?;

    vm_core::vm_success!("Database '{}' backed up to {:?}", db_name, backup_path);

    if retention_count > 0 {
        clean_old_backups(route, db_name, retention_count).await?;
    }

    Ok(())
}

/// Restore a database
pub async fn restore_db(
    route: &DbRoute,
    backup_name: &str,
    db_name: &str,
    yes: bool,
) -> VmResult<()> {
    reject_system_database(db_name)?;
    validate_backup_component(backup_name)?;
    let backup_path = get_backup_dir(route)?.join(backup_name);
    if !backup_name.starts_with(&format!("{db_name}_")) {
        return Err(VmError::validation(
            format!("Backup '{backup_name}' does not belong to database '{db_name}'"),
            None::<String>,
        ));
    }
    if !backup_path.exists() {
        return Err(VmError::validation(
            "Backup file not found",
            Some(format!("Backup file not found at: {backup_path:?}")),
        ));
    }

    if !confirm_destructive(
        &format!("Replace database '{db_name}' with backup '{backup_name}'?"),
        yes,
    )? {
        return Ok(());
    }

    let backup_data = tokio::fs::read(&backup_path)
        .await
        .map_err(|e| VmError::filesystem(e, backup_path.to_string_lossy(), "read"))?;

    // Validate the archive before touching any database.
    execute_docker_command(route, &["pg_restore", "--list"], Some(&backup_data)).await?;

    // Restore into a staging database so the current database stays intact if
    // validation or restoration fails.
    let operation_id = Uuid::new_v4().simple().to_string();
    let staging_name = format!("vm_restore_{operation_id}");
    let previous_name = format!("vm_previous_{operation_id}");
    create_database(route, &staging_name).await?;

    let restore_result = execute_docker_command(
        route,
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
        let _ = drop_database(route, &staging_name).await;
        return Err(error);
    }

    replace_database(route, &staging_name, db_name, &previous_name).await?;

    vm_core::vm_success!("Database '{}' restored from '{}'", db_name, backup_name);
    Ok(())
}

/// Export a database to a SQL file
pub async fn export_db(
    route: &DbRoute,
    db_name: &str,
    file: &Path,
    overwrite: bool,
) -> VmResult<()> {
    let output = execute_docker_command(
        route,
        &["pg_dump", "-U", "postgres", "-d", db_name, "--clean"],
        None,
    )
    .await?;

    let mut options = tokio::fs::OpenOptions::new();
    options.write(true);
    if overwrite {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut destination = options
        .open(file)
        .await
        .map_err(|e| VmError::filesystem(e, file.to_string_lossy(), "open"))?;
    use tokio::io::AsyncWriteExt;
    destination
        .write_all(&output)
        .await
        .map_err(|e| VmError::filesystem(e, file.to_string_lossy(), "write"))?;

    vm_core::vm_success!("Database '{}' exported to {:?}", db_name, file);
    Ok(())
}

/// Import a database from a SQL file
pub async fn import_db(route: &DbRoute, db_name: &str, file: &Path, yes: bool) -> VmResult<()> {
    reject_system_database(db_name)?;
    if !file.exists() {
        return Err(VmError::validation(
            "Import file not found",
            Some(format!("Import file not found at: {file:?}")),
        ));
    }

    if !confirm_destructive(
        &format!(
            "Import SQL into database '{db_name}' from '{}'?",
            file.display()
        ),
        yes,
    )? {
        return Ok(());
    }

    let sql_data = tokio::fs::read(file)
        .await
        .map_err(|e| VmError::filesystem(e, file.to_string_lossy(), "read"))?;

    execute_docker_command(
        route,
        &["psql", "-U", "postgres", "-d", db_name],
        Some(&sql_data),
    )
    .await?;

    vm_core::vm_success!("Database '{}' imported from {:?}", db_name, file);
    Ok(())
}

/// Reset a database
pub async fn reset_db(route: &DbRoute, db_name: &str, yes: bool) -> VmResult<()> {
    reject_system_database(db_name)?;
    if !confirm_destructive(&format!("Permanently reset database '{db_name}'?"), yes)? {
        return Ok(());
    }

    let operation_id = Uuid::new_v4().simple().to_string();
    let staging_name = format!("vm_reset_{operation_id}");
    let previous_name = format!("vm_previous_{operation_id}");
    create_database(route, &staging_name).await?;
    replace_database(route, &staging_name, db_name, &previous_name).await?;

    vm_core::vm_success!("Database '{}' has been reset.", db_name);
    Ok(())
}

fn confirm_destructive(prompt: &str, yes: bool) -> VmResult<bool> {
    if yes {
        return Ok(true);
    }
    vm_core::prompts::confirm_select(prompt, false).map_err(Into::into)
}

fn reject_system_database(name: &str) -> VmResult<()> {
    if matches!(name, "postgres" | "template0" | "template1") {
        return Err(VmError::validation(
            format!("System database '{name}' cannot be replaced"),
            None::<String>,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_values_are_quoted_as_one_identifier_or_literal() {
        assert_eq!(
            postgres::quote_pg_identifier("db\"; DROP DATABASE postgres; --"),
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
