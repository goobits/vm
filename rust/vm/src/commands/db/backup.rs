//! DB backup and restore logic
use crate::error::{VmError, VmResult};
use chrono::Local;
use std::path::{Path, PathBuf};

/// Get the base directory for backups
fn get_backup_dir() -> VmResult<PathBuf> {
    let backup_dir = vm_core::user_paths::home_dir()?
        .join("backups")
        .join("postgres");
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| VmError::filesystem(e, backup_dir.to_string_lossy(), "create_dir_all"))?;
    Ok(backup_dir)
}

/// Execute a command in the postgres docker container
async fn execute_docker_command(args: &[&str], input: Option<Vec<u8>>) -> VmResult<Vec<u8>> {
    let mut cmd = tokio::process::Command::new("docker");
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
        if let Err(e) = stdin.write_all(&input_data).await {
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

/// Backup a database
pub async fn backup_db(
    db_name: &str,
    backup_name: Option<&str>,
    retention_count: u32,
) -> VmResult<()> {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let backup_file_name = match backup_name {
        Some(name) => format!("{}_{}.dump", name, timestamp),
        None => format!("{}_{}.dump", db_name, timestamp),
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
    let backup_path = get_backup_dir()?.join(backup_name);
    if !backup_path.exists() {
        return Err(VmError::validation(
            "Backup file not found",
            Some(format!("Backup file not found at: {:?}", backup_path)),
        ));
    }

    let backup_data = tokio::fs::read(&backup_path)
        .await
        .map_err(|e| VmError::filesystem(e, backup_path.to_string_lossy(), "read"))?;

    // Drop and recreate the database before restoring
    execute_docker_command(
        &[
            "psql",
            "-U",
            "postgres",
            "-c",
            &format!("DROP DATABASE IF EXISTS \"{}\";", db_name),
        ],
        None,
    )
    .await?;
    execute_docker_command(
        &[
            "psql",
            "-U",
            "postgres",
            "-c",
            &format!("CREATE DATABASE \"{}\";", db_name),
        ],
        None,
    )
    .await?;

    execute_docker_command(
        &[
            "pg_restore",
            "-U",
            "postgres",
            "-d",
            db_name,
            "--clean",
            "--if-exists",
        ],
        Some(backup_data),
    )
    .await?;

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
            Some(format!("Import file not found at: {:?}", file)),
        ));
    }

    let sql_data = tokio::fs::read(file)
        .await
        .map_err(|e| VmError::filesystem(e, file.to_string_lossy(), "read"))?;

    execute_docker_command(&["psql", "-U", "postgres", "-d", db_name], Some(sql_data)).await?;

    vm_core::vm_success!("Database '{}' imported from {:?}", db_name, file);
    Ok(())
}

/// Reset a database
pub async fn reset_db(db_name: &str, force: bool) -> VmResult<()> {
    if !force {
        vm_core::vm_println!(
            "⚠️  This will permanently delete all data in the '{}' database.",
            db_name
        );
        print!("Are you sure you want to continue? (y/N) ");
        use std::io::{self, Write};
        io::stdout()
            .flush()
            .map_err(|e| VmError::general(e, "Failed to flush stdout"))?;
        let mut response = String::new();
        io::stdin()
            .read_line(&mut response)
            .map_err(|e| VmError::general(e, "Failed to read user input"))?;
        if response.trim().to_lowercase() != "y" {
            vm_core::vm_println!("Database reset cancelled.");
            return Ok(());
        }
    }

    execute_docker_command(
        &[
            "psql",
            "-U",
            "postgres",
            "-c",
            &format!("DROP DATABASE IF EXISTS \"{}\";", db_name),
        ],
        None,
    )
    .await?;

    execute_docker_command(
        &[
            "psql",
            "-U",
            "postgres",
            "-c",
            &format!("CREATE DATABASE \"{}\";", db_name),
        ],
        None,
    )
    .await?;

    vm_core::vm_success!("Database '{}' has been reset.", db_name);
    Ok(())
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
                .starts_with(&format!("{}_", db_name))
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
