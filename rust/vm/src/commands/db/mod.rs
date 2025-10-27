//! DB subcommand handlers

pub mod backup;
pub mod utils;

use crate::cli::DbSubcommand;
use crate::error::VmResult;
use vm_config::GlobalConfig;
use vm_core::vm_println;

async fn show_credentials(service_name: &str) -> VmResult<()> {
    let secrets_dir = vm_core::user_paths::secrets_dir()?;
    let secret_file = secrets_dir.join(format!("{}.env", service_name));

    if secret_file.exists() {
        let password = tokio::fs::read_to_string(secret_file).await?;
        vm_println!("Password for {}: {}", service_name, password.trim());
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
        DbSubcommand::Backup { db_name, name, all } => {
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

                vm_println!("📦 Backing up {} databases...", databases.len());
                let mut success_count = 0;
                let mut failed_count = 0;

                for db in databases {
                    match backup::backup_db(&db, None, global_config.backups.keep_count).await {
                        Ok(()) => {
                            success_count += 1;
                        }
                        Err(e) => {
                            vm_println!("⚠️  Failed to backup '{}': {}", db, e);
                            failed_count += 1;
                        }
                    }
                }

                vm_println!(
                    "\n✅ Backup complete: {} succeeded, {} failed",
                    success_count,
                    failed_count
                );
            } else if let Some(db) = db_name {
                backup::backup_db(&db, name.as_deref(), global_config.backups.keep_count).await?;
            } else {
                return Err(crate::error::VmError::validation(
                    "Missing database name",
                    Some(
                        "Provide a database name or use --all to backup all databases".to_string(),
                    ),
                ));
            }
        }
        DbSubcommand::Restore { name, db_name } => {
            backup::restore_db(&name, &db_name).await?;
        }
        DbSubcommand::List => {
            let result = utils::execute_psql_command(
                "SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datistemplate = false;",
            )
            .await?;

            vm_println!("📊 Databases:");
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
        DbSubcommand::Export { name, file } => {
            backup::export_db(&name, &file).await?;
        }
        DbSubcommand::Import { file, db_name } => {
            backup::import_db(&db_name, &file).await?;
        }
        DbSubcommand::Size => {
            let result = utils::execute_psql_command(
                "SELECT datname, pg_size_pretty(pg_database_size(datname)) FROM pg_database WHERE datistemplate = false;",
            )
            .await?;
            vm_println!("Database Sizes:");
            for line in result.lines() {
                let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                if parts.len() == 2 && !parts[0].is_empty() {
                    vm_println!("  - {:<30} {}", parts[0], parts[1]);
                }
            }
        }
        DbSubcommand::Reset { name, force } => {
            backup::reset_db(&name, force).await?;
        }
        DbSubcommand::Credentials { service } => {
            show_credentials(&service).await?;
        }
    }
    Ok(())
}
