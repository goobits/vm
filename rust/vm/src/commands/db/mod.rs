//! DB subcommand handlers

pub mod backup;
pub mod utils;

use crate::cli::{DbBackupSubcommand, DbSubcommand};
use crate::error::VmResult;
use vm_config::GlobalConfig;
use vm_core::{vm_println, vm_progress, vm_success, vm_warning};

async fn show_credentials(service_name: &str, reveal: bool) -> VmResult<()> {
    backup::validate_backup_component(service_name)?;
    let secrets_dir = vm_core::user_paths::secrets_dir()?;
    let secret_file = secrets_dir.join(format!("{}.env", service_name));

    if secret_file.exists() {
        if reveal {
            let password = tokio::fs::read_to_string(secret_file).await?;
            vm_println!("{}", password.trim_end());
        } else {
            vm_println!("Credentials for '{}': available (redacted)", service_name);
        }
    } else {
        vm_println!(
            "No credentials found for service '{}'. Has it been started yet?",
            service_name
        );
    }
    Ok(())
}

pub async fn handle_db(command: DbSubcommand) -> VmResult<()> {
    let global_config = GlobalConfig::load()?;

    match command {
        DbSubcommand::Backups {
            command:
                DbBackupSubcommand::Create {
                    name,
                    database,
                    all,
                },
        } => {
            if all {
                // Backup all databases except system ones
                let result = utils::execute_psql_command(
                    "SELECT datname FROM pg_database WHERE datistemplate = false AND datname NOT IN ('postgres');",
                )
                .await?;

                let databases: Vec<String> = result
                    .lines()
                    .map(|line| line.trim().to_string())
                    .filter(|db| !db.is_empty())
                    .collect();

                if databases.is_empty() {
                    vm_println!("No databases found to backup.");
                    return Ok(());
                }

                vm_progress!("Backing up {} databases...", databases.len());
                let mut success_count = 0;
                let mut failed_count = 0;

                for db in databases {
                    match backup::backup_db(&db, Some(&name), global_config.backups.keep_count)
                        .await
                    {
                        Ok(()) => {
                            success_count += 1;
                        }
                        Err(e) => {
                            vm_warning!("Failed to back up '{db}': {e}");
                            failed_count += 1;
                        }
                    }
                }

                if failed_count > 0 {
                    return Err(crate::error::VmError::validation(
                        format!(
                            "Database backup completed with {success_count} succeeded and {failed_count} failed"
                        ),
                        None::<String>,
                    ));
                }
                vm_success!("Backed up {success_count} database(s)");
            } else if let Some(db) = database {
                backup::backup_db(&db, Some(&name), global_config.backups.keep_count).await?;
            } else {
                return Err(crate::error::VmError::validation(
                    "Missing database name",
                    Some(
                        "Provide a database name or use --all to backup all databases".to_string(),
                    ),
                ));
            }
        }
        DbSubcommand::Backups {
            command:
                DbBackupSubcommand::Restore {
                    backup,
                    database,
                    yes,
                },
        } => {
            backup::restore_db(&backup, &database, yes).await?;
        }
        DbSubcommand::Backups {
            command: DbBackupSubcommand::List { database },
        } => {
            for item in backup::list_backups(database.as_deref())? {
                vm_println!("{item}");
            }
        }
        DbSubcommand::Backups {
            command: DbBackupSubcommand::Remove { backup, yes },
        } => {
            backup::remove_backup(&backup, yes)?;
        }
        DbSubcommand::List => {
            let result = utils::execute_psql_command(
                "SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datistemplate = false;",
            )
            .await?;

            vm_println!("Databases:");
            for line in result.lines() {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() == 2 && !parts[0].is_empty() {
                    let db_name = parts[0];
                    let db_size = parts[1];
                    let backup_count = backup::count_backups(db_name).await.unwrap_or(0);

                    if backup_count > 0 {
                        vm_println!(
                            "  - {:<30} {} ({} backup{})",
                            db_name,
                            db_size,
                            backup_count,
                            if backup_count == 1 { "" } else { "s" }
                        );
                    } else {
                        vm_println!("  - {:<30} {} (no backups)", db_name, db_size);
                    }
                }
            }

            if let Ok(backup_path) = backup::get_backup_path() {
                vm_println!("\n💾 Backups stored in: {}", backup_path);
            }
        }
        DbSubcommand::Status { name } => {
            let query = format!("SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datname = {};", backup::quote_pg_literal(&name));
            let result = utils::execute_psql_command(&query).await?;
            if result.trim().is_empty() {
                return Err(crate::error::VmError::validation(
                    format!("Database '{name}' not found"),
                    None::<String>,
                ));
            }
            vm_println!("{}", result.trim());
            vm_println!("Backups: {}", backup::count_backups(&name).await?);
        }
        DbSubcommand::Export {
            name,
            output,
            overwrite,
        } => {
            backup::export_db(&name, &output, overwrite).await?;
        }
        DbSubcommand::Import { name, file, yes } => {
            backup::import_db(&name, &file, yes).await?;
        }
        DbSubcommand::Reset { name, yes } => {
            backup::reset_db(&name, yes).await?;
        }
        DbSubcommand::Credentials { service, reveal } => {
            show_credentials(&service, reveal).await?;
        }
    }
    Ok(())
}
